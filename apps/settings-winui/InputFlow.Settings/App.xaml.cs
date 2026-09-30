using InputFlow.Settings.Core;
using Microsoft.UI.Xaml;

namespace InputFlow_Settings;

public partial class App : Application
{
    private Window? _window;
    private SettingsInstance? _instance;

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        _instance = SettingsInstance.TryAcquire();
        if (_instance is null)
        {
            SettingsInstance.ActivateExistingWindow();
            Environment.Exit(0);
            return;
        }

        var coordinator = new AgentConnectionCoordinator();
        _window = new MainWindow(coordinator, ReleaseInstance);
        _window.Activate();
    }

    private void ReleaseInstance()
    {
        _instance?.Dispose();
        _instance = null;
        _window = null;
    }
}
