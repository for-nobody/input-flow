using System.Globalization;
using InputFlow.Settings.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.UI;

namespace InputFlow_Settings;

public sealed partial class MainPage
{
    private enum PresentationOperation { Start, Stop, Mouse, Clear, Preset, NumpadStart, NumpadStop }
    private sealed record BindingEdit(PresentationOperation Operation, string? PresetId, string Label, bool Optional);

    private BindingEdit? _presentationBinding;
    private PresentationDocument? _presentationBeforeEdit;
    private PresentationDocument? _renderedPresentation;
    private string? _selectedPresentationPreset;
    private bool _refreshingPresentation;
    private IReadOnlyList<PresentationDisplay> _renderedDisplays = [];

    private bool Supports(string capability) =>
        _coordinator?.Handshake?.Capabilities.Contains(capability, StringComparer.Ordinal) == true;

    private void RefreshPresentationControls()
    {
        DoubleTapChoice.Visibility = Supports("key_double_tap") ? Visibility.Visible : Visibility.Collapsed;
        bool supported = Supports("presentation_pointer") && _draft.IsLoaded && _draft.Draft.SchemaVersion >= 6;
        bool visible = supported && _draft.Draft.Touchpad.ExperimentalEnabled;
        PresentationEnabled.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        PresentationEnabled.IsEnabled = !_busy;
        if (!visible)
        {
            PresentationCard.Visibility = Visibility.Collapsed;
            ClosePresentationEditor();
            return;
        }
        PresentationDocument draft = _draft.Draft.Presentation;
        _refreshingPresentation = true;
        PresentationEnabled.IsOn = draft.Enabled;
        _refreshingPresentation = false;
        if (!draft.Enabled)
        {
            PresentationCard.Visibility = Visibility.Collapsed;
            ClosePresentationEditor();
            return;
        }

        PresentationStatus? status = _coordinator?.LatestStatus?.Presentation;
        PresentationSummary.Text = AppResources.Format("Presentation_SummaryFormat",
            RuleRow.TriggerLabel(draft.Start), RuleRow.TriggerLabel(draft.Stop), draft.Presets.Count);
        string message = AppResources.Get((status?.Phase, status?.Reason) switch
        {
            (_, "ink_capacity") => "Presentation_CapacityStatus",
            ("pointing", _) => "Presentation_PointingStatus",
            ("drawing", _) => "Presentation_DrawingStatus",
            ("mouse", _) => "Presentation_MouseStatus",
            (_, "overlay_failed" or "cursor_unavailable") => "Presentation_OverlayStatus",
            (_, "numpad_active") => "Presentation_NumpadActive",
            (_, "paused") => "Presentation_PausedStatus",
            (_, "display_unavailable" or "display_changed") => "Presentation_DisplayStatus",
            (_, "device_unavailable" or "device_removed") => "Presentation_DeviceStatus",
            ("ready", _) => "Presentation_ReadyStatus",
            _ => "Presentation_DisabledStatus",
        });
        if (_coordinator?.Connection.ControlConnected != true)
            message = AppResources.Get("Touchpad_LastKnown") + " " + message;
        else if (_draft.IsDirty)
            message = AppResources.Get("Touchpad_Pending") + " " + message;
        if (PresentationState.Text != message) PresentationState.Text = message;
        PresentationCard.Visibility = PresentationEditor.Visibility == Visibility.Visible ? Visibility.Collapsed : Visibility.Visible;
        if (PresentationEditor.Visibility == Visibility.Visible) RenderPresentationEditor(draft, status?.Displays ?? []);
    }

    private void PresentationEnabled_Toggled(object sender, RoutedEventArgs e)
    {
        if (!_refreshingPresentation && _draft.IsLoaded)
        {
            _draft.SetPresentation(_draft.Draft.Presentation with { Enabled = PresentationEnabled.IsOn });
        }
    }

    private void PresentationEdit_Click(object sender, RoutedEventArgs e)
    {
        if (_busy || !_draft.IsLoaded || RuleEditor.Visibility == Visibility.Visible) return;
        PresentationDocument draft = _draft.Draft.Presentation;
        _presentationBeforeEdit = draft;
        _selectedPresentationPreset = draft.DefaultPreset;
        _renderedPresentation = null;
        PresentationEditor.Visibility = Visibility.Visible;
        RefreshPresentationControls();
        PresentationEditor.Focus(FocusState.Programmatic);
    }

    private void ClosePresentationEditor()
    {
        if (PresentationEditor.Visibility != Visibility.Visible) return;
        PresentationEditor.Visibility = Visibility.Collapsed;
        _presentationBeforeEdit = null;
        _renderedPresentation = null;
        if (_presentationBinding is not null)
        {
            RuleEditor.Visibility = Visibility.Collapsed;
            SetBindingEditor(null);
            UpdateResponsiveEditor();
        }
    }

    private void PresentationDone_Click(object sender, RoutedEventArgs e)
    {
        ClosePresentationEditor();
        RefreshPresentationControls();
    }

    private void PresentationDiscard_Click(object sender, RoutedEventArgs e)
    {
        if (_presentationBeforeEdit is { } original && _draft.IsLoaded)
            _draft.SetPresentation(original with { Enabled = _draft.Draft.Presentation.Enabled });
        PresentationDone_Click(sender, e);
    }

    private void RenderPresentationEditor(PresentationDocument draft, IReadOnlyList<PresentationDisplay> displays)
    {
        if (_renderedPresentation is { } previous
            && (previous with { Presets = draft.Presets }) == draft
            && previous.Presets.SequenceEqual(draft.Presets)
            && displays.SequenceEqual(_renderedDisplays)) return;
        _renderedPresentation = draft;
        _renderedDisplays = displays;
        _refreshingPresentation = true;
        PresentationBindings.Children.Clear();
        foreach (var (operation, title, trigger, optional) in new (PresentationOperation, string, RuleTrigger?, bool)[]
        {
            (PresentationOperation.Start, "Presentation_StartBinding", draft.Start, false),
            (PresentationOperation.Stop, "Presentation_StopBinding", draft.Stop, false),
            (PresentationOperation.Mouse, "Presentation_MouseBinding", draft.TemporaryMouse, true),
            (PresentationOperation.Clear, "Presentation_ClearBinding", draft.Clear, true),
        })
        {
            PresentationBindings.Children.Add(BindingButton(new(operation, null, AppResources.Get(title), optional), trigger));
        }

        PresentationPresetRows.Children.Clear();
        foreach (PresentationPreset preset in draft.Presets)
        {
            var row = new Grid { ColumnSpacing = 7 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var defaultPick = new RadioButton { GroupName = "PresentationDefault", IsChecked = preset.Id == draft.DefaultPreset, MinWidth = 28 };
            AutomationProperties.SetName(defaultPick, AppResources.Format("Presentation_DefaultFormat", preset.Name));
            defaultPick.Checked += (_, _) =>
            {
                if (!_refreshingPresentation) _draft.SetPresentation(_draft.Draft.Presentation with { DefaultPreset = preset.Id });
            };
            var miniature = new Border { Width = 44, Height = 32, Child = ShapePreview(preset.Style, 30) };
            Grid.SetColumn(miniature, 1);
            var select = new Button { Content = preset.Name, HorizontalAlignment = HorizontalAlignment.Stretch };
            AutomationProperties.SetName(select, AppResources.Format("Presentation_SelectFormat", preset.Name));
            select.Click += (_, _) =>
            {
                _selectedPresentationPreset = preset.Id;
                _renderedPresentation = null;
                RefreshPresentationControls();
                PresentationPresetName.Focus(FocusState.Programmatic);
            };
            Grid.SetColumn(select, 2);
            Button bind = BindingButton(new(PresentationOperation.Preset, preset.Id, preset.Name, true), preset.Binding);
            Grid.SetColumn(bind, 3);
            row.Children.Add(defaultPick);
            row.Children.Add(miniature);
            row.Children.Add(select);
            row.Children.Add(bind);
            PresentationPresetRows.Children.Add(row);
        }
        PresentationPreset? selected = draft.Presets.FirstOrDefault(preset => preset.Id == _selectedPresentationPreset)
            ?? draft.Presets.FirstOrDefault();
        _selectedPresentationPreset = selected?.Id;
        SelectedPresentationPreset.Visibility = selected is null ? Visibility.Collapsed : Visibility.Visible;
        PresentationDeletePresetButton.IsEnabled = selected is not null;
        if (selected is not null) PopulatePreset(selected);
        var choices = new List<LocalizedChoice<string>> { new("", AppResources.Get("Presentation_AutoDisplay")) };
        choices.AddRange(displays.Select((display, index) => new LocalizedChoice<string>(display.Id,
            AppResources.Format("Presentation_DisplayFormat", index + 1, display.Width, display.Height, display.Left, display.Top))));
        if (draft.Display is { } id && !choices.Any(choice => choice.Value == id))
            choices.Add(new(id, AppResources.Format("Presentation_MissingDisplayFormat", id)));
        PresentationDisplayTarget.ItemsSource = choices;
        SelectChoice(PresentationDisplayTarget, draft.Display ?? "");
        PresentationMovement.SelectedIndex = draft.Absolute ? 1 : 0;
        _refreshingPresentation = false;
    }

    private Button BindingButton(BindingEdit slot, RuleTrigger? value)
    {
        string binding = value is null ? AppResources.Get("Presentation_Unbound") : RuleRow.TriggerLabel(value);
        var button = new Button { Content = slot.Operation == PresentationOperation.Preset ? binding : $"{slot.Label} · {binding}" };
        AutomationProperties.SetName(button, AppResources.Format("Presentation_EditBindingFormat", slot.Label, binding));
        button.Click += (_, _) => OpenBindingEditor(slot, value);
        return button;
    }

    private void OpenBindingEditor(BindingEdit slot, RuleTrigger? value)
    {
        if (_busy || RuleEditor.Visibility == Visibility.Visible) return;
        SetBindingEditor(slot);
        EditorTitle.Text = AppResources.Format("Presentation_BindingTitleFormat", slot.Label);
        PopulateTriggerControls(value ?? new DoubleTapTrigger(KeyIdentity.Logical("A")));
        EditorInfo.IsOpen = false;
        RuleEditor.Visibility = Visibility.Visible;
        UpdateResponsiveEditor();
        TriggerTypeCombo.Focus(FocusState.Programmatic);
    }

    private void SetBindingEditor(BindingEdit? binding)
    {
        _presentationBinding = binding;
        RuleIdBox.Visibility = EditorEnabled.Visibility = RuleActionFields.Visibility =
            binding is null ? Visibility.Visible : Visibility.Collapsed;
        RemovePresentationBindingButton.Visibility = binding?.Optional == true ? Visibility.Visible : Visibility.Collapsed;
    }

    private void SavePresentationBinding(RuleTrigger? trigger)
    {
        if (_presentationBinding is not { } slot) return;
        if (slot.Operation is PresentationOperation.NumpadStart or PresentationOperation.NumpadStop)
        {
            if (trigger is not null)
            {
                NumpadDocument numpad = _draft.Draft.Numpad;
                _draft.SetNumpad(slot.Operation == PresentationOperation.NumpadStart
                    ? numpad with { Start = trigger } : numpad with { Stop = trigger });
            }
            return;
        }
        PresentationDocument draft = _draft.Draft.Presentation;
        _draft.SetPresentation(slot.Operation switch
        {
            PresentationOperation.Start when trigger is not null => draft with { Start = trigger },
            PresentationOperation.Stop when trigger is not null => draft with { Stop = trigger },
            PresentationOperation.Mouse => draft with { TemporaryMouse = trigger },
            PresentationOperation.Clear => draft with { Clear = trigger },
            PresentationOperation.Preset => draft with { Presets = draft.Presets.Select(preset => preset.Id == slot.PresetId ? preset with { Binding = trigger } : preset).ToArray() },
            _ => draft,
        });
    }

    private void PresentationRemoveBinding_Click(object sender, RoutedEventArgs e)
    {
        SavePresentationBinding(null);
        CancelEdit_Click(sender, e);
    }

    private void PresentationAddPreset_Click(object sender, RoutedEventArgs e)
    {
        PresentationDocument draft = _draft.Draft.Presentation;
        if (draft.Presets.Count >= 32) return;
        var preset = new PresentationPreset(Guid.NewGuid().ToString("N"), AppResources.Get("Presentation_NewPreset"), null,
            new PointerAppearance(PointerStyle.Ring, "#FF3333", 28, 3, 5, null));
        _selectedPresentationPreset = preset.Id;
        _draft.SetPresentation(draft with {
            Presets = [.. draft.Presets, preset], DefaultPreset = draft.Presets.Count == 0 ? preset.Id : draft.DefaultPreset,
        });
    }

    private void PresentationDeletePreset_Click(object sender, RoutedEventArgs e)
    {
        PresentationDocument draft = _draft.Draft.Presentation;
        PresentationPreset[] remaining = draft.Presets.Where(preset => preset.Id != _selectedPresentationPreset).ToArray();
        _selectedPresentationPreset = remaining.FirstOrDefault()?.Id;
        _draft.SetPresentation(draft with {
            Presets = remaining,
            DefaultPreset = remaining.Any(preset => preset.Id == draft.DefaultPreset) ? draft.DefaultPreset : _selectedPresentationPreset ?? "",
        });
    }

    private void PresentationMovePreset_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string tag } || !int.TryParse(tag, out int delta)) return;
        PresentationDocument draft = _draft.Draft.Presentation;
        List<PresentationPreset> order = draft.Presets.ToList();
        int from = order.FindIndex(preset => preset.Id == _selectedPresentationPreset);
        int to = from + delta;
        if (from < 0 || to < 0 || to >= order.Count) return;
        (order[from], order[to]) = (order[to], order[from]);
        _draft.SetPresentation(draft with { Presets = order });
    }

    private void PopulatePreset(PresentationPreset preset)
    {
        var style = preset.Style;
        PresentationPresetName.Text = preset.Name;
        PresentationShape.SelectedIndex = (int)style.Type;
        PresentationHexColor.Text = style.Color;
        PresentationColorPickerControl.Color = ReadPreviewColor(style.Color);
        PresentationDiameter.Value = style.Diameter;
        PresentationRingWidth.Value = style.Thickness;
        PresentationInkWidth.Value = style.Width;
        PresentationTimedInk.IsOn = style.RetainSeconds.HasValue;
        PresentationRetention.Value = style.RetainSeconds ?? 3;
        PresentationDiameter.Visibility = style.Type != PointerStyle.Ink ? Visibility.Visible : Visibility.Collapsed;
        PresentationRingWidth.Visibility = style.Type == PointerStyle.Ring ? Visibility.Visible : Visibility.Collapsed;
        PresentationTimedInk.Visibility = PresentationInkWidth.Visibility = style.Type == PointerStyle.Ink ? Visibility.Visible : Visibility.Collapsed;
        PresentationRetention.Visibility = style.Type == PointerStyle.Ink && style.RetainSeconds.HasValue ? Visibility.Visible : Visibility.Collapsed;
        if (PresentationSwatches.Children.Count == 0)
        {
            foreach (string color in new[] { "#FF0000", "#FFFF00", "#00FF00", "#00FFFF", "#0000FF", "#FF00FF", "#000000", "#FFFFFF" })
            {
                var swatch = new Button { Width = 26, Height = 26, MinWidth = 0, Padding = new Thickness(0),
                    Background = new SolidColorBrush(ReadPreviewColor(color)) };
                AutomationProperties.SetName(swatch, AppResources.Format("Presentation_SwatchFormat", color));
                swatch.Click += (_, _) => { PresentationHexColor.Text = color; ApplyPresetFields(); };
                PresentationSwatches.Children.Add(swatch);
            }
        }
        RenderLargePreview(style);
    }

    private static Color ReadPreviewColor(string value)
    {
        if (value.Length != 7 || value[0] != '#'
            || !uint.TryParse(value.AsSpan(1), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out uint rgb))
            rgb = 0xFF3333;
        return Color.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
    }

    private static FrameworkElement ShapePreview(PointerAppearance style, double max)
    {
        var brush = new SolidColorBrush(ReadPreviewColor(style.Color));
        if (style.Type == PointerStyle.Ink)
        {
            return new Microsoft.UI.Xaml.Shapes.Path
            {
                Data = new PathGeometry { Figures = { new PathFigure { StartPoint = new(0, 18),
                    Segments = { new BezierSegment { Point1 = new(12, 1), Point2 = new(22, 31), Point3 = new(36, 10) } } } } },
                Stroke = brush, StrokeThickness = Math.Min(style.Width, max / 2),
                StrokeStartLineCap = PenLineCap.Round, StrokeEndLineCap = PenLineCap.Round,
                HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center,
            };
        }
        double diameter = Math.Min(style.Diameter, max);
        return new Ellipse { Width = diameter, Height = diameter,
            Fill = style.Type == PointerStyle.Dot ? brush : null,
            Stroke = style.Type == PointerStyle.Ring ? brush : null,
            StrokeThickness = Math.Min(style.Thickness, diameter / 2),
            HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
    }

    private void RenderLargePreview(PointerAppearance style)
    {
        PresentationPreview.Background = new SolidColorBrush(ReadPreviewColor(PresentationPreviewDark.IsOn ? "#202020" : "#F4F4F4"));
        PresentationPreview.Child = ShapePreview(style, 64);
    }

    private void ApplyPresetFields()
    {
        if (_refreshingPresentation || !_draft.IsLoaded || _selectedPresentationPreset is null) return;
        if (double.IsNaN(PresentationDiameter.Value) || double.IsNaN(PresentationRingWidth.Value)
            || double.IsNaN(PresentationInkWidth.Value) || double.IsNaN(PresentationRetention.Value)
            || PresentationShape.SelectedIndex < 0) return;
        PresentationDocument draft = _draft.Draft.Presentation;
        var style = new PointerAppearance((PointerStyle)PresentationShape.SelectedIndex,
            PresentationHexColor.Text.Trim(), (uint)PresentationDiameter.Value, (uint)PresentationRingWidth.Value,
            (uint)PresentationInkWidth.Value, PresentationTimedInk.IsOn ? (uint)PresentationRetention.Value : null);
        _draft.SetPresentation(draft with { Presets = draft.Presets.Select(preset => preset.Id == _selectedPresentationPreset
            ? preset with { Name = PresentationPresetName.Text.Trim(), Style = style } : preset).ToArray() });
    }

    private void PresentationShape_Changed(object sender, SelectionChangedEventArgs e) => ApplyPresetFields();
    private void PresentationPresetFields_Changed(object sender, RoutedEventArgs e) => ApplyPresetFields();
    private void PresentationNumber_Changed(NumberBox sender, NumberBoxValueChangedEventArgs args) => ApplyPresetFields();
    private void PresentationTimedInk_Toggled(object sender, RoutedEventArgs e) => ApplyPresetFields();

    private void PresentationColor_Changed(ColorPicker sender, ColorChangedEventArgs args)
    {
        if (_refreshingPresentation) return;
        PresentationHexColor.Text = $"#{args.NewColor.R:X2}{args.NewColor.G:X2}{args.NewColor.B:X2}";
        ApplyPresetFields();
    }

    private void PresentationPreview_Changed(object sender, RoutedEventArgs e)
    {
        if (!_refreshingPresentation && _draft.IsLoaded
            && _draft.Draft.Presentation.Presets.FirstOrDefault(preset => preset.Id == _selectedPresentationPreset) is { } selected)
            RenderLargePreview(selected.Style);
    }

    private void PresentationDisplay_Changed(object sender, SelectionChangedEventArgs e)
    {
        if (!_refreshingPresentation && _draft.IsLoaded && PresentationDisplayTarget.SelectedItem is LocalizedChoice<string> display)
            _draft.SetPresentation(_draft.Draft.Presentation with { Display = display.Value == "" ? null : display.Value });
    }

    private void PresentationMovement_Changed(object sender, SelectionChangedEventArgs e)
    {
        if (!_refreshingPresentation && _draft.IsLoaded && PresentationMovement.SelectedIndex >= 0)
            _draft.SetPresentation(_draft.Draft.Presentation with { Absolute = PresentationMovement.SelectedIndex == 1 });
    }

    private async void PresentationIdentify_Click(object sender, RoutedEventArgs e)
    {
        if (_coordinator?.Connection.ControlConnected != true) return;
        try { await _coordinator.IdentifyDisplaysAsync(_pageLifetime.Token); }
        catch (Exception error) { ShowMessage(AppResources.Get("Presentation_DisplayStatus"), FriendlyError(error), InfoBarSeverity.Error); }
    }
}
