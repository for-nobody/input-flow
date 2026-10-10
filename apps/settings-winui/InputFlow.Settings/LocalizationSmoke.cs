using InputFlow.Settings.Core;
using Microsoft.Windows.Globalization;

namespace InputFlow_Settings;

internal static class LocalizationSmoke
{
    private const string Switch = "--localization-smoke";
    private const string EnvironmentVariable = "INPUTFLOW_LOCALIZATION_SMOKE";

    public static string? GetRequestedEnvironmentLanguage()
    {
        string? language = Environment.GetEnvironmentVariable(EnvironmentVariable);
        return language is UiLanguagePreference.EnglishUnitedStates or UiLanguagePreference.ChineseSimplified
            ? language
            : null;
    }

    public static string? GetRequestedLanguage(IReadOnlyList<string> arguments)
    {
        int index = arguments.IndexOf(Switch);
        if (index < 0 || index + 1 >= arguments.Count)
        {
            return null;
        }

        string language = arguments[index + 1];
        return language is UiLanguagePreference.EnglishUnitedStates or UiLanguagePreference.ChineseSimplified
            ? language
            : null;
    }

    public static string? GetRequestedLanguage(string arguments)
    {
        string[] tokens = arguments.Split(
            [' ', '\t'],
            StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        return GetRequestedLanguage(tokens);
    }

    public static int? TryRun(IReadOnlyList<string> arguments)
    {
        if (!arguments.Contains(Switch, StringComparer.Ordinal))
        {
            return null;
        }

        string? language = GetRequestedLanguage(arguments);
        if (language is null)
        {
            return 2;
        }

        return Run(language);
    }

    public static int? TryRun(string launchArguments)
    {
        string[] arguments = launchArguments.Split(
            [' ', '\t'],
            StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (!arguments.Contains(Switch, StringComparer.Ordinal))
        {
            return null;
        }

        string? language = GetRequestedLanguage(arguments);
        if (language is null)
        {
            return 2;
        }

        ApplicationLanguages.PrimaryLanguageOverride = language;
        return Run(language);
    }

    public static int? TryRunEnvironment()
    {
        string? language = GetRequestedEnvironmentLanguage();
        return language is null ? null : Run(language);
    }

    private static int Run(string language)
    {
        try
        {
            var probe = new LocalizationProbe();
            var page = new MainPage();
            string expectedStatic = language == UiLanguagePreference.EnglishUnitedStates ? "Localization ready" : "本地化已就绪";
            string expectedRules = language == UiLanguagePreference.EnglishUnitedStates ? "Rules" : "快捷规则";
            string expectedDynamic = language == UiLanguagePreference.EnglishUnitedStates ? "Preview stopped." : "预览已停止。";
            string expectedAutomation = language == UiLanguagePreference.EnglishUnitedStates
                ? "Localization smoke button"
                : "本地化冒烟测试按钮";
            bool passed = AppResources.CurrentLanguageTag == language &&
                string.Equals(probe.StaticText, expectedStatic, StringComparison.Ordinal) &&
                string.Equals(page.LocalizationSmokeRulesText, expectedRules, StringComparison.Ordinal) &&
                string.Equals(AppResources.Get("Preview_Stopped"), expectedDynamic, StringComparison.Ordinal) &&
                string.Equals(probe.AutomationName, expectedAutomation, StringComparison.Ordinal);
            return passed ? 0 : 1;
        }
        catch
        {
            return 1;
        }
    }

    private static int IndexOf(this IReadOnlyList<string> values, string value)
    {
        for (int index = 0; index < values.Count; index++)
        {
            if (string.Equals(values[index], value, StringComparison.Ordinal))
            {
                return index;
            }
        }

        return -1;
    }
}
