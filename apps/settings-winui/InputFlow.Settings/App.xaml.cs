using InputFlow.Settings.Core;
using Microsoft.Windows.Globalization;
using Microsoft.UI.Xaml;
using System.Diagnostics;

namespace InputFlow_Settings;

public partial class App : Application
{
    private Window? _window;
    private SettingsInstance? _instance;

    internal static UiLanguagePreferenceStore LanguagePreferenceStore { get; } =
        UiLanguagePreferenceStore.CreateDefault();

    internal static string LanguagePreference { get; private set; } = UiLanguagePreference.System;

    public App()
    {
        string? smokeLanguage = LocalizationSmoke.GetRequestedEnvironmentLanguage() ??
            LocalizationSmoke.GetRequestedLanguage(Environment.CommandLine);
        UiLanguagePreferenceLoadResult load = LanguagePreferenceStore.Load();
        LanguagePreference = smokeLanguage ?? load.Language;
        if (load.UsedFallback)
        {
            Debug.WriteLine($"InputFlow UI preference fallback: {load.DiagnosticCode}");
        }

        string? languageOverride = LanguagePreference switch
        {
            UiLanguagePreference.EnglishUnitedStates => UiLanguagePreference.EnglishUnitedStates,
            UiLanguagePreference.ChineseSimplified => UiLanguagePreference.ChineseSimplified,
            _ => null,
        };
        if (languageOverride is not null)
        {
            ApplicationLanguages.PrimaryLanguageOverride = languageOverride;
        }
        InitializeComponent();
    }

    protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        int? smokeExitCode = LocalizationSmoke.TryRunEnvironment() ??
            LocalizationSmoke.TryRun($"{Environment.CommandLine} {args.Arguments}");
        if (smokeExitCode is int exitCode)
        {
            Environment.Exit(exitCode);
            return;
        }

        _instance = SettingsInstance.TryAcquire();
        if (_instance is null)
        {
            SettingsInstance.ActivateExistingWindow(AppResources.Get("Window_Title"));
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
