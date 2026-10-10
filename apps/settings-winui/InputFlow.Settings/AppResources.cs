using System.Globalization;
using Microsoft.Windows.ApplicationModel.Resources;

namespace InputFlow_Settings;

internal static class AppResources
{
    private static readonly Lazy<ResourceLoader> Loader = new(() => new ResourceLoader());

    public static string Get(string key)
    {
        string value = Loader.Value.GetString(key);
        return string.IsNullOrWhiteSpace(value)
            ? "A required interface resource is unavailable."
            : value;
    }

    public static string Format(string key, params object?[] arguments) =>
        string.Format(CultureInfo.CurrentCulture, Get(key), arguments);

    public static string CurrentLanguageTag => Get("LanguageTag");
}
