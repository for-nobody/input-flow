using InputFlow.Settings.Core;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Windows.Graphics;

namespace InputFlow_Settings;

public sealed partial class MainWindow : Window
{
    private readonly AgentConnectionCoordinator _coordinator;
    private readonly Action _releaseInstance;
    private bool _closingPromptActive;
    private bool _allowClose;

    public MainWindow(AgentConnectionCoordinator coordinator, Action releaseInstance)
    {
        _coordinator = coordinator;
        _releaseInstance = releaseInstance;
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.SetIcon("Assets/AppIcon.ico");
        AppWindow.Resize(new SizeInt32(1100, 720));
        AppWindow.Closing += AppWindow_Closing;
        Closed += MainWindow_Closed;
        RootFrame.Navigate(typeof(MainPage), coordinator);
    }

    private async void AppWindow_Closing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        if (_allowClose)
        {
            return;
        }

        args.Cancel = true;
        if (_closingPromptActive)
        {
            return;
        }

        _closingPromptActive = true;
        try
        {
            bool close = RootFrame.Content is not MainPage page || await page.PrepareToCloseAsync();
            if (close)
            {
                _allowClose = true;
                Close();
            }
        }
        finally
        {
            _closingPromptActive = false;
        }
    }

    private async void MainWindow_Closed(object sender, WindowEventArgs args)
    {
        AppWindow.Closing -= AppWindow_Closing;
        Closed -= MainWindow_Closed;
        try
        {
            Task dispose = _coordinator.DisposeAsync().AsTask();
            Task completed = await Task.WhenAny(dispose, Task.Delay(TimeSpan.FromSeconds(2)));
            if (ReferenceEquals(completed, dispose))
            {
                await dispose;
            }
        }
        catch
        {
            // Closing is already committed. The agent owns all durable state,
            // and disposing the settings process is the remaining guarantee.
        }
        finally
        {
            // A broken/closing pipe must never keep the WinUI process alive
            // after its final window has gone away.
            _releaseInstance();
        }
    }
}
