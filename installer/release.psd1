@{
    Version = '0.9.1'
    PublishFiles = @(
        'InputFlow.Protocol.dll',
        'InputFlow.Settings.Core.dll',
        'InputFlow.Settings.deps.json',
        'InputFlow.Settings.dll',
        'InputFlow.Settings.exe',
        'InputFlow.Settings.pri',
        'InputFlow.Settings.runtimeconfig.json',
        'Microsoft.InteractiveExperiences.Projection.dll',
        'Microsoft.Security.Authentication.OAuth.Projection.dll',
        'Microsoft.Web.WebView2.Core.dll',
        'Microsoft.Web.WebView2.Core.Projection.dll',
        'Microsoft.Windows.ApplicationModel.Background.Projection.dll',
        'Microsoft.Windows.ApplicationModel.Background.UniversalBGTask.dll',
        'Microsoft.Windows.ApplicationModel.DynamicDependency.Projection.dll',
        'Microsoft.Windows.ApplicationModel.Resources.Projection.dll',
        'Microsoft.Windows.ApplicationModel.WindowsAppRuntime.Projection.dll',
        'Microsoft.Windows.AppLifecycle.Projection.dll',
        'Microsoft.Windows.AppNotifications.Builder.Projection.dll',
        'Microsoft.Windows.AppNotifications.Projection.dll',
        'Microsoft.Windows.BadgeNotifications.Projection.dll',
        'Microsoft.Windows.Foundation.Projection.dll',
        'Microsoft.Windows.Management.Deployment.Projection.dll',
        'Microsoft.Windows.Media.Capture.Projection.dll',
        'Microsoft.Windows.PushNotifications.Projection.dll',
        'Microsoft.Windows.SDK.NET.dll',
        'Microsoft.Windows.Security.AccessControl.Projection.dll',
        'Microsoft.Windows.Storage.Pickers.Projection.dll',
        'Microsoft.Windows.Storage.Projection.dll',
        'Microsoft.Windows.System.Power.Projection.dll',
        'Microsoft.Windows.System.Projection.dll',
        'Microsoft.WindowsAppRuntime.Bootstrap.dll',
        'Microsoft.WindowsAppRuntime.Bootstrap.Net.dll',
        'Microsoft.WinUI.dll',
        'WebView2Loader.dll',
        'WinRT.Runtime.dll'
    )
    RustLicenses = @(
        @{ Name = 'itoa'; Version = '1.0.18'; File = 'LICENSE-MIT' },
        @{ Name = 'memchr'; Version = '2.8.3'; File = 'LICENSE-MIT' },
        @{ Name = 'serde'; Version = '1.0.229'; File = 'LICENSE-MIT' },
        @{ Name = 'serde_core'; Version = '1.0.229'; File = 'LICENSE-MIT' },
        @{ Name = 'serde_json'; Version = '1.0.151'; File = 'LICENSE-MIT' },
        @{ Name = 'windows-link'; Version = '0.2.1'; File = 'license-mit' },
        @{ Name = 'windows-sys'; Version = '0.61.2'; File = 'license-mit' },
        @{ Name = 'zmij'; Version = '1.0.23'; File = 'LICENSE-MIT' }
    )
    NuGetLicenses = @(
        @{ Id = 'Microsoft.Web.WebView2'; Version = '1.0.3719.77'; Files = @('LICENSE.txt', 'NOTICE.txt') },
        @{ Id = 'Microsoft.WindowsAppSDK.Foundation'; Version = '2.3.12'; Files = @('license.txt') },
        @{ Id = 'Microsoft.WindowsAppSDK.InteractiveExperiences'; Version = '2.1.9'; Files = @('license.txt') },
        @{ Id = 'Microsoft.WindowsAppSDK.Runtime'; Version = '2.5.1'; Files = @('license.txt', 'NOTICE.txt') },
        @{ Id = 'Microsoft.WindowsAppSDK.WinUI'; Version = '2.3.9'; Files = @('license.txt', 'NOTICE.txt') }
    )
}
