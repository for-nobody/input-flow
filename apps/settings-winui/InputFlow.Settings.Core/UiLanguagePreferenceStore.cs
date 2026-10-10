using System.Text.Json;

namespace InputFlow.Settings.Core;

public static class UiLanguagePreference
{
    public const string System = "system";
    public const string EnglishUnitedStates = "en-US";
    public const string ChineseSimplified = "zh-CN";

    public static bool IsSupported(string? value) =>
        value is System or EnglishUnitedStates or ChineseSimplified;
}

public sealed record UiLanguagePreferenceLoadResult(
    string Language,
    bool UsedFallback,
    string? DiagnosticCode);

public sealed class UiLanguagePreferenceStore
{
    private readonly string _path;

    public UiLanguagePreferenceStore(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        _path = Path.GetFullPath(path);
    }

    public static UiLanguagePreferenceStore CreateDefault()
    {
        string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return new UiLanguagePreferenceStore(Path.Combine(localAppData, "InputFlow", "ui-preferences.json"));
    }

    public UiLanguagePreferenceLoadResult Load()
    {
        if (!File.Exists(_path))
        {
            return new(UiLanguagePreference.System, false, null);
        }

        try
        {
            using JsonDocument document = JsonDocument.Parse(File.ReadAllBytes(_path));
            if (document.RootElement.ValueKind != JsonValueKind.Object ||
                !document.RootElement.TryGetProperty("language", out JsonElement languageElement) ||
                languageElement.ValueKind != JsonValueKind.String)
            {
                return new(UiLanguagePreference.System, true, "invalid_shape");
            }

            string? language = languageElement.GetString();
            return UiLanguagePreference.IsSupported(language)
                ? new(language!, false, null)
                : new(UiLanguagePreference.System, true, "unsupported_language");
        }
        catch (JsonException)
        {
            return new(UiLanguagePreference.System, true, "invalid_json");
        }
        catch (IOException)
        {
            return new(UiLanguagePreference.System, true, "read_failed");
        }
        catch (UnauthorizedAccessException)
        {
            return new(UiLanguagePreference.System, true, "access_denied");
        }
    }

    public void Save(string language)
    {
        if (!UiLanguagePreference.IsSupported(language))
        {
            throw new ArgumentOutOfRangeException(nameof(language), language, "Unsupported UI language preference.");
        }

        string directory = Path.GetDirectoryName(_path)
            ?? throw new InvalidOperationException("The UI preference path has no parent directory.");
        Directory.CreateDirectory(directory);
        string temporaryPath = Path.Combine(directory, $".{Path.GetFileName(_path)}.{Guid.NewGuid():N}.tmp");
        try
        {
            byte[] payload = JsonSerializer.SerializeToUtf8Bytes(
                new UiPreferences(language),
                new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower, WriteIndented = true });
            using (var stream = new FileStream(
                temporaryPath,
                FileMode.CreateNew,
                FileAccess.Write,
                FileShare.None,
                4096,
                FileOptions.WriteThrough))
            {
                stream.Write(payload);
                stream.WriteByte((byte)'\n');
                stream.Flush(flushToDisk: true);
            }

            File.Move(temporaryPath, _path, overwrite: true);
        }
        finally
        {
            try
            {
                File.Delete(temporaryPath);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
    }

    private sealed record UiPreferences(string Language);
}
