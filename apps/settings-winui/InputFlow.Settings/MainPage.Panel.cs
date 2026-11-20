using InputFlow.Settings.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace InputFlow_Settings;

public sealed partial class MainPage
{
    private readonly Dictionary<string, PanelCanvasWindow> _panelEditors = new(StringComparer.Ordinal);
    private bool _managingPanel;
    private string? _viewedLayout;
    private VirtualPanelDocument? _renderedPanel;
    private bool _renderedPanelBusy;
    private PanelBoard? _panelPreview;
    private TextBlock? _panelDimensions;
    private Button? _editPanelCanvas;
    private TextBlock? _panelNotice;
    private NumpadStatus? PanelStatus => _coordinator?.LatestStatus?.Numpad;

    private VirtualPanelDocument? CurrentPanel => _draft.IsLoaded && _draft.Draft.SchemaVersion >= 8
        ? _draft.Draft.Numpad.Panel : null;

    private void SetPanel(VirtualPanelDocument panel)
    {
        _draft.SetNumpad(_draft.Draft.Numpad with { Panel = panel });
        RefreshPanelManagement();
    }

    private void PanelManage_Click(object sender, RoutedEventArgs e)
    {
        if (_busy || RuleEditor.Visibility == Visibility.Visible) return;
        ShowPanelManagement(true);
    }

    private void ShowPanelManagement(bool visible)
    {
        if (_managingPanel == visible) return;
        _managingPanel = visible;
        if (!visible && _presentationBinding?.Operation == PresentationOperation.PanelSelect) CancelRuleEdit();
        PanelManagement.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        RulesList.Visibility = visible ? Visibility.Collapsed : Visibility.Visible;
        if (visible)
        {
            _renderedPanel = null;
            RefreshPanelManagement();
        }
    }

    private void PanelManageGrid_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        bool narrow = e.NewSize.Width < 630;
        PanelSidebarColumn.Width = narrow ? new GridLength(0) : new GridLength(248);
        Grid.SetColumn(PanelSidebar, narrow ? 0 : 1);
        Grid.SetRow(PanelDetails, narrow ? 1 : 0);
    }

    private void RefreshPanelManagement()
    {
        if (!_managingPanel || CurrentPanel is not { } panel) return;
        if (!ReferenceEquals(panel, _renderedPanel) || _renderedPanelBusy != _busy)
        {
            _renderedPanel = panel;
            _renderedPanelBusy = _busy;
            _viewedLayout = panel.Layouts.Any(layout => layout.Id == _viewedLayout)
                ? _viewedLayout : panel.DefaultLayout ?? panel.Layouts.FirstOrDefault()?.Id;
            RenderPanelManager(panel);
        }
        RefreshPanelSize();
    }

    private void RefreshPanelSize()
    {
        NumpadStatus? status = PanelStatus;
        _panelPreview?.SetDeviceSize(status?.WidthMm, status?.HeightMm);
        if (_panelDimensions is not null) _panelDimensions.Text = PanelBoard.DimensionsText(status?.WidthMm, status?.HeightMm);
    }

    private void RenderPanelManager(VirtualPanelDocument panel)
    {
        PanelListContent.Children.Clear();
        PanelSidebar.Header = AppResources.Get("Panel_Layouts");

        var list = new ListView { SelectionMode = ListViewSelectionMode.Single, MaxHeight = 320 };
        AutomationProperties.SetName(list, AppResources.Get("Panel_Layouts"));
        foreach (PanelLayout layout in panel.Layouts)
        {
            string display = layout.Id == panel.DefaultLayout
                ? AppResources.Format("Panel_DefaultFormat", LayoutName(layout)) : LayoutName(layout);
            var item = new ListViewItem { Content = display, Tag = layout.Id };
            list.Items.Add(item);
            if (layout.Id == _viewedLayout) list.SelectedItem = item;
        }
        list.SelectionChanged += (_, _) =>
        {
            if (list.SelectedItem is not ListViewItem { Tag: string id } || id == _viewedLayout) return;
            _viewedLayout = id;
            if (CurrentPanel is { } current)
            {
                RenderPanelDetails(current);
                RefreshPanelSize();
            }
        };
        PanelListContent.Children.Add(list);
        var add = PanelButton("Panel_New", async () =>
        {
            string? name = await AskLayoutNameAsync("Panel_New", AppResources.Get("Panel_NewDefaultName"));
            if (name is null || CurrentPanel is not { } current || current.Layouts.Count >= 24) return;
            PanelLayout layout = PanelLayout.Blank(name);
            _viewedLayout = layout.Id;
            SetPanel(current.Add(layout));
        });
        add.IsEnabled = !_busy && panel.Layouts.Count < 24;
        PanelListContent.Children.Add(add);

        Button presets = PanelButton("Panel_AddPreset", () => { });
        var flyout = new MenuFlyout();
        foreach (PanelLayout preset in VirtualPanelDocument.CreatePresets())
        {
            var item = new MenuFlyoutItem { Text = LayoutName(preset) };
            item.Click += (_, _) => AddPanelPreset(preset);
            flyout.Items.Add(item);
        }
        presets.Flyout = flyout;
        presets.IsEnabled = !_busy && panel.Layouts.Count < 24;
        PanelListContent.Children.Add(presets);

        RenderPanelDetails(panel);
    }

    private void RenderPanelDetails(VirtualPanelDocument panel)
    {
        PanelDetails.Children.Clear();
        _editPanelCanvas = null;
        _panelPreview = null;
        _panelDimensions = null;
        PanelDetails.Children.Add(PanelButton("Panel_Back", () => ShowPanelManagement(false)));
        PanelDetails.Children.Add(new TextBlock { Text = AppResources.Get("Panel_Manage"), FontSize = 22 });
        _panelNotice = new TextBlock { TextWrapping = TextWrapping.Wrap };
        AutomationProperties.SetLiveSetting(_panelNotice, AutomationLiveSetting.Polite);
        PanelDetails.Children.Add(_panelNotice);
        PanelLayout? selected = panel.Layouts.FirstOrDefault(layout => layout.Id == _viewedLayout);
        if (selected is null)
        {
            PanelDetails.Children.Add(new TextBlock { Text = AppResources.Get("Panel_NoLayouts"), TextWrapping = TextWrapping.Wrap });
            return;
        }

        PanelDetails.Children.Add(new TextBlock { Text = LayoutName(selected), FontSize = 19, TextWrapping = TextWrapping.Wrap });
        PanelDetails.Children.Add(new TextBlock
        {
            Text = AppResources.Get(selected.Id == panel.DefaultLayout ? "Panel_Default" : "Panel_NotDefault"),
        });
        Button binding = BindingButton(new(PresentationOperation.PanelSelect, selected.Id, AppResources.Get("Panel_Switch"), true), selected.Select);
        binding.IsEnabled = !_busy;
        PanelDetails.Children.Add(binding);
        var actions = new CommandBar { DefaultLabelPosition = CommandBarDefaultLabelPosition.Right };
        void ActionButton(string key, Symbol icon, Action action, bool enabled = true)
        {
            var button = new AppBarButton { Label = AppResources.Get(key), Icon = new SymbolIcon(icon), IsEnabled = enabled && !_busy };
            button.Click += (_, _) => action();
            actions.PrimaryCommands.Add(button);
        }
        ActionButton("Panel_Rename", Symbol.Rename, async () =>
        {
            string? name = await AskLayoutNameAsync("Panel_Rename", LayoutName(selected));
            UpdatePanelLayout(selected.Id, current => name is null ? current : current with { Name = name });
        });
        ActionButton("Panel_Copy", Symbol.Copy, () =>
        {
            if (CurrentPanel is not { } current || current.Layouts.Count >= 24) return;
            string name = AppResources.Format("Panel_CopyFormat", LayoutName(selected));
            PanelLayout copy = selected.Copy(name[..Math.Min(name.Length, 80)]);
            _viewedLayout = copy.Id;
            SetPanel(current.Add(copy));
        }, panel.Layouts.Count < 24);
        ActionButton("Panel_Delete", Symbol.Delete, async () =>
        {
            var dialog = new ContentDialog
            {
                XamlRoot = XamlRoot, Title = AppResources.Get("Panel_Delete"),
                Content = AppResources.Format("Panel_DeleteConfirm", LayoutName(selected)),
                PrimaryButtonText = AppResources.Get("Panel_Delete"), CloseButtonText = AppResources.Get("Common_Back"),
            };
            if (await dialog.ShowAsync() == ContentDialogResult.Primary && CurrentPanel is { } current)
                SetPanel(current.Remove(selected.Id));
        });
        ActionButton("Panel_SetDefault", Symbol.Pin, () =>
        {
            if (CurrentPanel is { } current) SetPanel(current with { DefaultLayout = selected.Id });
        }, selected.Id != panel.DefaultLayout);
        PanelDetails.Children.Add(actions);

        _panelPreview = new PanelBoard(labels: false) { Height = 170, MaxWidth = 370, HorizontalAlignment = HorizontalAlignment.Stretch };
        _panelPreview.Render(selected.Geometry);
        PanelDetails.Children.Add(_panelPreview);
        _panelDimensions = new TextBlock();
        PanelDetails.Children.Add(_panelDimensions);
        _editPanelCanvas = PanelButton("Panel_EditCanvas", () => OpenPanelCanvas(selected));
        _editPanelCanvas.IsEnabled = !_busy;
        PanelDetails.Children.Add(_editPanelCanvas);
    }

    private static Button PanelButton(string key, Action action)
    {
        var button = new Button { Content = AppResources.Get(key) };
        button.Click += (_, _) => action();
        return button;
    }

    private static string LayoutName(PanelLayout layout) => layout.Name switch
    {
        "Numpad" => AppResources.Get("Panel_PresetNumbers"),
        "Classic numpad" => AppResources.Get("Panel_LegacyTitle"),
        "Media" => AppResources.Get("Panel_PresetMedia"),
        "Presentation controls" => AppResources.Get("Panel_PresetSlides"),
        _ => layout.Name,
    };

    private void AddPanelPreset(PanelLayout preset)
    {
        if (CurrentPanel is not { } panel || panel.Layouts.Count >= 24) return;
        ConfigDocument config = _draft.Draft;
        var existing = config.Rules.Where(rule => rule.Enabled).Select(rule => rule.Trigger)
            .Concat(panel.Layouts.Select(layout => layout.Select).OfType<RuleTrigger>())
            .Concat([config.Numpad.Start, config.Numpad.Stop, config.Presentation.Start, config.Presentation.Stop]);
        bool conflict = preset.Select is KeyChordTrigger chord &&
            (chord.First == config.EmergencyBypassKey || chord.Second == config.EmergencyBypassKey ||
            existing.Any(trigger => trigger == chord || trigger.FirstKey == chord.First
                && trigger is HoldTrigger or HoldMouseButtonTrigger or MouseDirectionTrigger));
        PanelLayout layout = preset.Copy(preset.Name) with { Select = conflict ? null : preset.Select };
        _viewedLayout = layout.Id;
        SetPanel(panel.Add(layout));
        if (conflict && _panelNotice is not null) _panelNotice.Text = AppResources.Get("Panel_PresetShortcutConflict");
    }

    private void UpdatePanelLayout(string id, Func<PanelLayout, PanelLayout> update)
    {
        if (CurrentPanel is { } panel && panel.Layouts.FirstOrDefault(layout => layout.Id == id) is { } layout)
        {
            PanelLayout updated = update(layout);
            if (updated != layout) SetPanel(panel.Replace(updated));
        }
    }

    private async Task<string?> AskLayoutNameAsync(string title, string initial)
    {
        var name = new TextBox { Text = initial, Header = AppResources.Get("Panel_Name"), MaxLength = 80 };
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot, Title = AppResources.Get(title), Content = name,
            PrimaryButtonText = AppResources.Get("Panel_Done"), CloseButtonText = AppResources.Get("Common_Back"),
            DefaultButton = ContentDialogButton.Primary, IsPrimaryButtonEnabled = PanelGeometryEditor.ValidText(initial, 80),
        };
        name.TextChanged += (_, _) => dialog.IsPrimaryButtonEnabled = PanelGeometryEditor.ValidText(name.Text, 80);
        return await dialog.ShowAsync() == ContentDialogResult.Primary ? name.Text.Trim() : null;
    }

    private void OpenPanelCanvas(PanelLayout layout)
    {
        if (_panelEditors.TryGetValue(layout.Id, out PanelCanvasWindow? existing))
        {
            existing.BringForward();
            return;
        }
        var window = new PanelCanvasWindow(layout, LayoutName(layout), PanelStatus?.WidthMm, PanelStatus?.HeightMm,
            geometry => SavePanelCanvas(layout, geometry), picker => StartCaptureAsync(picker, mouseTarget: false),
            CancelCaptureBestEffortAsync);
        _panelEditors.Add(layout.Id, window);
        window.Closed += (_, _) =>
        {
            _panelEditors.Remove(layout.Id);
            if (!_closingPanelEditors && _managingPanel)
            {
                App.ActivateMainWindow();
                _editPanelCanvas?.Focus(FocusState.Programmatic);
            }
        };
        window.Activate();
    }

    private string? SavePanelCanvas(PanelLayout original, PanelGeometry geometry)
    {
        string? error = _draft.SavePanelCanvas(original, geometry, _busy);
        if (error is null) RefreshPanelManagement();
        return error;
    }

    private bool _closingPanelEditors;

    private async Task<bool> ClosePanelEditorsAsync()
    {
        _closingPanelEditors = true;
        try
        {
            foreach (PanelCanvasWindow window in _panelEditors.Values.ToArray())
                if (!await window.ConfirmCloseAsync()) return false;
            return true;
        }
        finally
        {
            _closingPanelEditors = false;
        }
    }
}
