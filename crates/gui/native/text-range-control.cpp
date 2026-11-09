// Private cross-process reference client using the official Windows UIA SDK.
// It observes the same live textbox; it neither sends input nor prints its text.
#include <windows.h>
#include <UIAutomation.h>
#include <wrl/client.h>
#include <cstdio>
#include <cstdlib>
#include <cwchar>
#include <cstdint>

using Microsoft::WRL::ComPtr;

static bool checked(const char* stage, HRESULT result) {
    std::printf("SDK %s HRESULT=0x%08lX\n", stage, static_cast<unsigned long>(result));
    std::fflush(stdout);
    return SUCCEEDED(result);
}

static bool text_length(const char* stage, IUIAutomationTextRange* range) {
    BSTR text = nullptr;
    std::printf("SDK %s pending\n", stage);
    std::fflush(stdout);
    const HRESULT result = range->GetText(-1, &text);
    const bool valid = checked(stage, result);
    if (valid) {
        std::printf("SDK %s length=%u\n", stage, SysStringLen(text));
        std::fflush(stdout);
    }
    SysFreeString(text);
    return valid;
}

static int inspect(HWND window) {
    ComPtr<IUIAutomation> automation;
    if (!checked("CoCreateInstance", CoCreateInstance(__uuidof(CUIAutomation), nullptr,
            CLSCTX_INPROC_SERVER, __uuidof(IUIAutomation),
            reinterpret_cast<void**>(automation.GetAddressOf()))) || !automation.Get()) return 1;
    ComPtr<IUIAutomationElement> root_element;
    if (!checked("ElementFromHandle", automation->ElementFromHandle(window, root_element.GetAddressOf())) || !root_element.Get()) return 1;
    VARIANT label;
    VariantInit(&label);
    label.vt = VT_BSTR;
    label.bstrVal = SysAllocString(L"Custom recipe notes");
    if (!label.bstrVal) return 1;
    ComPtr<IUIAutomationCondition> condition;
    const HRESULT condition_result = automation->CreatePropertyCondition(UIA_NamePropertyId,
        label, condition.GetAddressOf());
    VariantClear(&label);
    if (!checked("CreatePropertyCondition", condition_result) || !condition.Get()) return 1;
    ComPtr<IUIAutomationElement> notes;
    if (!checked("FindFirst", root_element->FindFirst(TreeScope_Descendants, condition.Get(),
            notes.GetAddressOf())) || !notes.Get()) return 1;
    ComPtr<IUIAutomationTextPattern> pattern;
    if (!checked("GetCurrentPatternAs", notes->GetCurrentPatternAs(UIA_TextPatternId,
            __uuidof(IUIAutomationTextPattern), reinterpret_cast<void**>(pattern.GetAddressOf()))) || !pattern.Get()) return 1;
    ComPtr<IUIAutomationTextRange> document;
    if (!checked("DocumentRange", pattern->get_DocumentRange(document.GetAddressOf()))
            || !document.Get() || !text_length("Document.GetText", document.Get())) return 1;
    ComPtr<IUIAutomationTextRangeArray> selection;
    if (!checked("GetSelection", pattern->GetSelection(selection.GetAddressOf())) || !selection.Get()) return 1;
    int count = 0;
    if (!checked("Selection.length", selection->get_Length(&count))) return 1;
    std::printf("SDK selection count=%d\n", count);
    std::fflush(stdout);
    if (count != 1) return 1;
    ComPtr<IUIAutomationTextRange> selected;
    if (!checked("Selection.GetElement", selection->GetElement(0, selected.GetAddressOf())) || !selected.Get()) return 1;
    int start = 0, end = 0;
    if (!checked("Selection.CompareStart", selected->CompareEndpoints(TextPatternRangeEndpoint_Start,
            document.Get(), TextPatternRangeEndpoint_Start, &start))) return 1;
    if (!checked("Selection.CompareEnd", selected->CompareEndpoints(TextPatternRangeEndpoint_End,
            document.Get(), TextPatternRangeEndpoint_End, &end))) return 1;
    std::printf("SDK selection start_vs_document_start=%d end_vs_document_end=%d\n", start, end);
    std::fflush(stdout);
    return text_length("Selection.GetText", selected.Get()) ? 0 : 1;
}

int wmain(int argc, wchar_t** argv) {
    if (argc != 2) return 2;
    wchar_t* end = nullptr;
    const unsigned long long raw = std::wcstoull(argv[1], &end, 10);
    if (!raw || !end || *end) return 2;
    const HRESULT initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (!checked("CoInitializeEx.MTA", initialized)) return 1;
    const int result = inspect(reinterpret_cast<HWND>(static_cast<std::uintptr_t>(raw)));
    CoUninitialize();
    return result;
}
