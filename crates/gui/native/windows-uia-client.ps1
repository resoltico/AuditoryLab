# Official installed Framework client-side provider setup, before any UIA lookup.
# This registers Microsoft's stock proxies; it supplies no application proxy.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName UIAutomationClientsideProviders
$nativeProxyAssembly = [AppDomain]::CurrentDomain.GetAssemblies() |
    Where-Object { $_.GetName().Name -ieq 'UIAutomationClientsideProviders' } |
    Select-Object -First 1
if ($null -eq $nativeProxyAssembly) { throw 'Required installed Framework client-side provider assembly is unavailable' }
# Framework's default-proxy version discovery walks declaring types on its
# caller stack. A normal typed non-inlined caller avoids anonymous PS frames.
Add-Type -ReferencedAssemblies ([Windows.Automation.ClientSettings].Assembly.Location) -TypeDefinition @'
using System.Reflection;
using System.Runtime.CompilerServices;
public static class NativeLegacyUiaStartup {
    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void Initialize(AssemblyName providers) {
        System.Windows.Automation.ClientSettings.RegisterClientSideProviderAssembly(providers);
    }
}
'@
try {
    [NativeLegacyUiaStartup]::Initialize($nativeProxyAssembly.GetName())
} catch {
    Write-Host "Official legacy client prerequisite failed: $($_.Exception.InnerException)"
    throw
}
Write-Host "Native legacy stock proxies: $($nativeProxyAssembly.FullName) Location=$($nativeProxyAssembly.Location)"
