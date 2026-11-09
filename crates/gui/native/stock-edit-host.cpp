// Task-owned reference using the exact stock Win32 EDIT class and OS provider.
#include <windows.h>
#include <objbase.h>

static LRESULT CALLBACK window_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    if (message == WM_DESTROY) { PostQuitMessage(0); return 0; }
    if (message == WM_SETFOCUS) {
        const HWND edit = GetDlgItem(window, 100);
        if (edit) SetFocus(edit);
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

static int run(HINSTANCE instance, int show) {
    const wchar_t* class_name = L"AuditoryLabNativeReference";
    WNDCLASSW descriptor = {};
    descriptor.lpfnWndProc = window_proc;
    descriptor.hInstance = instance;
    descriptor.lpszClassName = class_name;
    descriptor.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    descriptor.hbrBackground = reinterpret_cast<HBRUSH>(static_cast<INT_PTR>(COLOR_WINDOW + 1));
    if (!RegisterClassW(&descriptor)) return 1;
    const HWND window = CreateWindowExW(0, class_name, L"Native UIA stock reference",
        WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 700, 400,
        nullptr, nullptr, instance, nullptr);
    if (!window) return 1;
    if (!CreateWindowExW(0, L"STATIC", L"Custom recipe notes", WS_CHILD | WS_VISIBLE,
            20, 15, 640, 20, window, reinterpret_cast<HMENU>(static_cast<INT_PTR>(101)), instance, nullptr)) return 1;
    const HWND edit = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT", L"",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
        20, 40, 640, 300, window, reinterpret_cast<HMENU>(static_cast<INT_PTR>(100)), instance, nullptr);
    if (!edit) return 1;
    SendMessageW(edit, WM_SETFONT, reinterpret_cast<WPARAM>(GetStockObject(DEFAULT_GUI_FONT)), TRUE);
    ShowWindow(window, show);
    UpdateWindow(window);
    SetFocus(edit);
    MSG message = {};
    int status;
    while ((status = GetMessageW(&message, nullptr, 0, 0)) > 0) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    return status < 0 ? 1 : 0;
}

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE, PWSTR, int show) {
    const HRESULT initialized = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    if (FAILED(initialized)) return 1;
    const int result = run(instance, show);
    CoUninitialize();
    return result;
}
