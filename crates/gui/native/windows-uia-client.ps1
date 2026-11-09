# Official installed Framework client-side provider setup, before any UIA lookup.
# This registers Microsoft's stock proxies; it supplies no application proxy.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName UIAutomationClientsideProviders
$nativeProxyAssembly = [AppDomain]::CurrentDomain.GetAssemblies() |
    Where-Object { $_.GetName().Name -ieq 'UIAutomationClientsideProviders' } |
    Select-Object -First 1
if ($null -eq $nativeProxyAssembly) { throw 'Required installed Framework client-side provider assembly is unavailable' }
[Windows.Automation.ClientSettings]::RegisterClientSideProviderAssembly($nativeProxyAssembly.GetName())
Write-Host "Native legacy stock proxies: $($nativeProxyAssembly.FullName) Location=$($nativeProxyAssembly.Location)"
