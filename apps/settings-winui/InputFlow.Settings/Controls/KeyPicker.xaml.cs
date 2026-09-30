using InputFlow.Settings.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace InputFlow_Settings.Controls;

public sealed class KeyOption
{
    public KeyOption()
    {
    }

    public KeyOption(string stableName, string label)
    {
        StableName = stableName;
        Label = label;
    }

    public string StableName { get; set; } = string.Empty;

    public string Label { get; set; } = string.Empty;
}

public sealed partial class KeyPicker : UserControl
{
    public KeyPicker()
    {
        Options = InputCatalog.LogicalKeys
            .Select(key =>
            {
                string display = KeyboardNameService.DisplayName(KeyIdentity.Logical(key));
                return new KeyOption(key, display == key ? key : $"{display} — {key}");
            })
            .ToList();
        InitializeComponent();
        LogicalKeyCombo.SelectedIndex = Math.Max(0, Options.FindIndex(option => option.StableName == "A"));
        UpdateIdentityText();
    }

    public event EventHandler? CaptureRequested;

    public List<KeyOption> Options { get; }

    public bool CaptureEnabled
    {
        get => CaptureButton.IsEnabled;
        set => CaptureButton.IsEnabled = value;
    }

    public bool TryGetIdentity(out KeyIdentity? identity, out string? error)
    {
        if (PhysicalMode.IsOn)
        {
            if (double.IsNaN(ScanCodeBox.Value) || ScanCodeBox.Value is < 1 or > 65535)
            {
                identity = null;
                error = "Physical scan code 必须在 1–65535 之间。";
                return false;
            }

            identity = KeyIdentity.Physical((ushort)ScanCodeBox.Value, ExtendedCheck.IsChecked == true);
            error = null;
            return true;
        }

        if (LogicalKeyCombo.SelectedItem is not KeyOption option)
        {
            identity = null;
            error = "请选择逻辑键。";
            return false;
        }

        identity = KeyIdentity.Logical(option.StableName);
        error = null;
        return true;
    }

    public void SetIdentity(KeyIdentity identity)
    {
        if (identity.Mode == KeyMatchMode.Logical)
        {
            LogicalKeyCombo.SelectedItem = Options.FirstOrDefault(option => option.StableName == identity.LogicalKey);
            PhysicalMode.IsOn = false;
        }
        else
        {
            ScanCodeBox.Value = identity.ScanCode;
            ExtendedCheck.IsChecked = identity.Extended;
            PhysicalMode.IsOn = true;
        }

        UpdateIdentityText();
    }

    public void SetCaptured(CapturedKey captured)
    {
        LogicalKeyCombo.SelectedItem = Options.FirstOrDefault(option => option.StableName == captured.Logical);
        if (captured.ScanCode is ushort scanCode && scanCode != 0)
        {
            ScanCodeBox.Value = scanCode;
            ExtendedCheck.IsChecked = captured.Extended;
        }

        // ADR-005: capture defaults to logical. The observed physical identity
        // remains available through the advanced toggle.
        PhysicalMode.IsOn = false;
        UpdateIdentityText();
    }

    private void CaptureButton_Click(object sender, RoutedEventArgs e) =>
        CaptureRequested?.Invoke(this, EventArgs.Empty);

    private void LogicalKeyCombo_SelectionChanged(object sender, SelectionChangedEventArgs e) =>
        UpdateIdentityText();

    private void PhysicalMode_Toggled(object sender, RoutedEventArgs e)
    {
        ScanCodeBox.IsEnabled = PhysicalMode.IsOn;
        ExtendedCheck.IsEnabled = PhysicalMode.IsOn;
        LogicalKeyCombo.IsEnabled = !PhysicalMode.IsOn;
        UpdateIdentityText();
    }

    private void UpdateIdentityText()
    {
        if (!TryGetIdentity(out KeyIdentity? identity, out string? error))
        {
            IdentityText.Text = error;
            return;
        }

        IdentityText.Text = $"显示：{KeyboardNameService.DisplayName(identity!)}　Identity：{identity!.StableName}";
    }
}
