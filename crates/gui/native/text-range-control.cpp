// Private cross-process reference client using the official Windows UIA SDK.
// It observes the same live textbox; it neither sends input nor prints its text.
#include <windows.h>
#include <UIAutomation.h>
#include <wrl/client.h>
#include <cstdio>
#include <cstdlib>
#include <cwchar>
#include <cstring>
#include <cstdint>

using Microsoft::WRL::ComPtr;

static bool checked(const char* stage, HRESULT result) {
    std::printf("SDK %s HRESULT=0x%08lX\n", stage, static_cast<unsigned long>(result));
    std::fflush(stdout);
    return SUCCEEDED(result);
}

struct Text {
    BSTR value = nullptr;
    ~Text() { SysFreeString(value); }
    Text() = default;
    Text(const Text&) = delete;
    Text& operator=(const Text&) = delete;
};

static bool read_text(const char* stage, IUIAutomationTextRange* range, Text& text) {
    std::printf("SDK %s pending\n", stage);
    std::fflush(stdout);
    const HRESULT result = range->GetText(-1, &text.value);
    const bool valid = checked(stage, result);
    if (valid) {
        std::printf("SDK %s length=%u\n", stage, SysStringLen(text.value));
        std::fflush(stdout);
    }
    return valid;
}

static int inspect(HWND window, bool full_selection) {
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
    Text document_text;
    if (!checked("DocumentRange", pattern->get_DocumentRange(document.GetAddressOf()))
            || !document.Get() || !read_text("Document.GetText", document.Get(), document_text)) return 1;
    // Independent literal oracle for the actual native keyboard input recipe.
    constexpr wchar_t expected[] = L"first native line\nsecond native line";
    constexpr size_t expected_length = sizeof(expected) / sizeof(expected[0]) - 1;
    const bool document_matches = SysStringLen(document_text.value) == expected_length
        && std::memcmp(document_text.value, expected, expected_length * sizeof(wchar_t)) == 0;
    if (!checked("Document.ExactExpectedText", document_matches ? S_OK : E_FAIL)) return 1;
    ComPtr<IUIAutomationTextRangeArray> selection;
    if (!checked("GetSelection", pattern->GetSelection(selection.GetAddressOf())) || !selection.Get()) return 1;
    int count = 0;
    if (!checked("Selection.length", selection->get_Length(&count))) return 1;
    std::printf("SDK selection count=%d\n", count);
    std::fflush(stdout);
    if (count != 1) return 1;
    ComPtr<IUIAutomationTextRange> selected;
    if (!checked("Selection.GetElement", selection->GetElement(0, selected.GetAddressOf())) || !selected.Get()) return 1;
    int start = 0, start_at_end = 0, end = 0;
    if (!checked("Selection.CompareStart", selected->CompareEndpoints(TextPatternRangeEndpoint_Start,
            document.Get(), TextPatternRangeEndpoint_Start, &start))) return 1;
    if (!checked("Selection.CompareStartToDocumentEnd", selected->CompareEndpoints(TextPatternRangeEndpoint_Start,
            document.Get(), TextPatternRangeEndpoint_End, &start_at_end))) return 1;
    if (!checked("Selection.CompareEnd", selected->CompareEndpoints(TextPatternRangeEndpoint_End,
            document.Get(), TextPatternRangeEndpoint_End, &end))) return 1;
    std::printf("SDK selection start_vs_document_start=%d start_vs_document_end=%d end_vs_document_end=%d\n", start, start_at_end, end);
    std::fflush(stdout);
    Text selected_text;
    if (!read_text("Selection.GetText", selected.Get(), selected_text)) return 1;
    const UINT document_length = SysStringLen(document_text.value);
    const UINT selected_length = SysStringLen(selected_text.value);
    const bool state_matches = full_selection
        ? start == 0 && end == 0 && document_length > 0 && document_length == selected_length
            && std::memcmp(document_text.value, selected_text.value, document_length * sizeof(wchar_t)) == 0
        : start_at_end == 0 && end == 0 && selected_length == 0;
    checked(full_selection ? "ExpectedFullSelection" : "ExpectedEmptyCaret", state_matches ? S_OK : E_PENDING);
    return state_matches ? 0 : 3;

}

int wmain(int argc, wchar_t** argv) {
    if (argc != 3) return 2;
    const bool full_selection = std::wcscmp(argv[2], L"selection") == 0;
    if (!full_selection && std::wcscmp(argv[2], L"caret") != 0) return 2;
    wchar_t* end = nullptr;
    const unsigned long long raw = std::wcstoull(argv[1], &end, 10);
    if (!raw || !end || *end) return 2;
    const HRESULT initialized = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (!checked("CoInitializeEx.MTA", initialized)) return 1;
    int result;
    const ULONGLONG deadline = GetTickCount64() + 5000;
    do {
        result = inspect(reinterpret_cast<HWND>(static_cast<std::uintptr_t>(raw)), full_selection);
        if (result != 3) break;
        Sleep(50);
    } while (GetTickCount64() < deadline);
    CoUninitialize();
    return result == 3 ? 1 : result;
}
