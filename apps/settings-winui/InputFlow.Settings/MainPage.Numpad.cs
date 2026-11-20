using InputFlow.Settings.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace InputFlow_Settings;

public sealed partial class MainPage
{
    private bool _refreshingNumpad;

    private void RefreshNumpadControls()
    {
        ConfigDocument? draft = _draft.IsLoaded ? _draft.Draft : null;
        bool visible = Supports("touchpad_numpad") && draft is { SchemaVersion: >= 7, Touchpad.ExperimentalEnabled: true };
        NumpadEnabled.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        bool showCard = visible && draft!.Numpad.Enabled;
        bool virtualPanel = Supports("touchpad_virtual_panel") && draft is { SchemaVersion: >= 8 };
        NumpadCardTitle.Text = AppResources.Get(virtualPanel ? "Panel_Title" : "Panel_LegacyTitle");
        NumpadEnabled.Header = NumpadCardTitle.Text;
        AutomationProperties.SetName(NumpadEnabled, (string)NumpadEnabled.Header);
        PanelManageButton.Visibility = virtualPanel ? Visibility.Visible : Visibility.Collapsed;
        PanelManageButton.Content = AppResources.Get("Panel_Manage");
        PanelManageButton.IsEnabled = !_busy;
        NumpadCard.Visibility = showCard ? Visibility.Visible : Visibility.Collapsed;
        if (!showCard && _presentationBinding?.Operation is PresentationOperation.NumpadStart or PresentationOperation.NumpadStop or PresentationOperation.PanelSelect)
        {
            CancelRuleEdit();
        }
        if (!showCard || !virtualPanel) ShowPanelManagement(false);
        else RefreshPanelManagement();
        if (!visible) return;

        _refreshingNumpad = true;
        NumpadDocument config = draft!.Numpad;
        NumpadEnabled.IsOn = config.Enabled;
        NumpadEnabled.IsEnabled = !_busy;
        NumpadGuideChoice.SelectedIndex = (int)config.Guide;
        NumpadGuideChoice.IsEnabled = NumpadStart.IsEnabled = NumpadStop.IsEnabled = !_busy;
        _refreshingNumpad = false;
        if (!showCard) return;

        UpdateNumpadBinding(NumpadStart, AppResources.Get(virtualPanel ? "Panel_Start" : "Numpad_Start"), config.Start);
        UpdateNumpadBinding(NumpadStop, AppResources.Get(virtualPanel ? "Panel_Stop" : "Numpad_Stop"), config.Stop);
        string guideLabel = AppResources.Get(virtualPanel ? "Panel_Guide" : "Panel_LegacyGuide");
        AutomationProperties.SetName(NumpadGuideChoice, guideLabel);
        NumpadStatus? status = _coordinator?.LatestStatus?.Numpad;
        string state = AppResources.Get(status?.Reason switch
        {
            "no_layout" => "Panel_NoLayouts",
            "ready" when virtualPanel && status.Phase == "active" => "Panel_Active",
            "ready" when virtualPanel => "Panel_Ready",
            "native_tap_enabled" when virtualPanel => "Panel_NativeTap",
            "native_tap_unknown" when virtualPanel => "Panel_NativeTapUnknown",
            "ready" when status.Phase == "active" => "Numpad_Active",
            "ready" => "Numpad_Ready",
            "presentation_active" when virtualPanel => "Panel_PresentationActive",
            "presentation_active" => "Numpad_PresentationActive",
            "native_tap_enabled" => "Numpad_NativeTap",
            "native_tap_unknown" => "Numpad_NativeTapUnknown",
            "focus_unavailable" when virtualPanel => "Panel_FocusUnavailable",
            "focus_unavailable" => "Numpad_FocusUnavailable",
            "focus_watch_unavailable" when virtualPanel => "Panel_FocusWatchUnavailable",
            "focus_watch_unavailable" => "Numpad_FocusWatchUnavailable",
            "overlay_failed" when virtualPanel => "Panel_OverlayFailed",
            "overlay_failed" => "Numpad_OverlayFailed",
            "output_fault" when virtualPanel => "Panel_OutputFault",
            "output_fault" => "Numpad_OutputFault",
            "device_unavailable" or "device_read_only" when virtualPanel => "Panel_DeviceUnavailable",
            "device_unavailable" or "device_read_only" => "Numpad_DeviceUnavailable",
            "paused" when virtualPanel => "Panel_Paused",
            "paused" => "Numpad_Paused",
            "recording" when virtualPanel => "Panel_RecordingMode",
            "recording" => "Numpad_Recording",
            "session_inactive" => "Numpad_SessionInactive",
            "recovery_waiting" => "Numpad_RecoveryWaiting",
            "config_pending" => "Touchpad_ConfigPending",
            "disabled" or null => "Numpad_Disabled",
            _ => "Numpad_Unavailable",
        });
        if (_coordinator?.Connection.ControlConnected != true)
            state = AppResources.Get("Touchpad_LastKnown") + " " + state;
        if (config != _coordinator?.LatestConfig?.Numpad || draft.Touchpad.ExperimentalEnabled != _coordinator?.LatestConfig?.Touchpad.ExperimentalEnabled)
            state = AppResources.Get("Touchpad_Pending") + " " + state;
        if (NumpadState.Text != state) NumpadState.Text = state;
        NumpadCounts.Text = status is null ? string.Empty : AppResources.Format(
            "Numpad_Counts", status.Confirmed, status.Cancelled, status.InsertedInputs);
    }

    private static void UpdateNumpadBinding(Button button, string label, RuleTrigger trigger)
    {
        string key = RuleRow.TriggerLabel(trigger);
        button.Content = $"{label} · {key}";
        AutomationProperties.SetName(button, AppResources.Format("Presentation_EditBindingFormat", label, key));
    }

    private void NumpadEnabled_Toggled(object sender, RoutedEventArgs e)
    {
        if (!_refreshingNumpad && _draft.IsLoaded)
            _draft.SetNumpad(_draft.Draft.Numpad with { Enabled = NumpadEnabled.IsOn });
    }

    private void NumpadGuide_Changed(object sender, SelectionChangedEventArgs e)
    {
        if (!_refreshingNumpad && _draft.IsLoaded && NumpadGuideChoice.SelectedIndex >= 0)
            _draft.SetNumpad(_draft.Draft.Numpad with { Guide = (NumpadGuide)NumpadGuideChoice.SelectedIndex });
    }

    private void NumpadBinding_Click(object sender, RoutedEventArgs e)
    {
        if (!_draft.IsLoaded) return;
        NumpadDocument config = _draft.Draft.Numpad;
        bool start = ReferenceEquals(sender, NumpadStart);
        bool panel = Supports("touchpad_virtual_panel");
        string label = AppResources.Get(panel
            ? (start ? "Panel_Start" : "Panel_Stop") : (start ? "Numpad_Start" : "Numpad_Stop"));
        OpenBindingEditor(new BindingEdit(
            start ? PresentationOperation.NumpadStart : PresentationOperation.NumpadStop,
            null, label, false),
            start ? config.Start : config.Stop);
    }
}
