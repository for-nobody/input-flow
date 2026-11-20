using System.Diagnostics;
using InputFlow.Settings.Core;
using InputFlow_Settings.Controls;
using Microsoft.UI.Input;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Windows.Graphics;
using Windows.System;
using Windows.UI.Core;

namespace InputFlow_Settings;

internal sealed class PanelCanvasWindow : Window
{
    private enum Tool { Select, Rectangle, Line }

    private sealed record Drag(PanelPoint Start, string? Region, PanelEdgeHandle? Edge, bool Touch, long Started);

    private readonly PanelHistory _history;
    private readonly Func<PanelGeometry, string?> _save;
    private readonly Func<KeyPicker, Task> _capture;
    private readonly Func<Task> _cancelCapture;
    private readonly PanelBoard _board = new(labels: true);
    // Keep the board's size and coordinate mapping stable as drag feedback changes.
    private readonly TextBlock _status = new()
    {
        TextWrapping = TextWrapping.Wrap,
        TextTrimming = TextTrimming.WordEllipsis,
        LineStackingStrategy = LineStackingStrategy.BlockLineHeight,
        LineHeight = 20,
        MinHeight = 40,
        MaxLines = 2,
    };
    private readonly ComboBox _tools = new();
    private readonly ComboBox _lineScope = new();
    private readonly ListView _regions = new() { SelectionMode = ListViewSelectionMode.Extended, MaxHeight = 115 };
    private readonly HashSet<string> _selected = new(StringComparer.Ordinal);
    private readonly Button _saveButton;
    private readonly Button _undo;
    private readonly Button _redo;
    private readonly AppBarButton _edit;
    private readonly AppBarButton _merge;
    private readonly AppBarButton _delete;
    private PanelSurface _surface;
    private PanelEdgeHandle? _hotEdge;
    private Drag? _drag;
    private PanelEdit? _preview;
    private bool _updatingRegionList;
    private bool _dialogOpen;
    private bool _closing;
    private bool _allowClose;
    private bool _recording;
    private IList<KeyPicker>? _dialogPickers;
    private TextBlock? _recordingStatus;

    public PanelCanvasWindow(PanelLayout layout, string name, double? width, double? height,
        Func<PanelGeometry, string?> save, Func<KeyPicker, Task> capture, Func<Task> cancelCapture)
    {
        _history = new(layout.Geometry);
        _surface = new(layout.Geometry);
        _save = save;
        _capture = capture;
        _cancelCapture = cancelCapture;
        Title = AppResources.Format("Panel_WindowTitle", name);
        AppWindow.Resize(new SizeInt32(990, 775));
        AppWindow.Closing += WindowClosing;
        Closed += (_, _) => AppWindow.Closing -= WindowClosing;

        var root = new Grid { Padding = new Thickness(14), RowSpacing = 5 };
        if (Application.Current.Resources.TryGetValue("ApplicationPageBackgroundThemeBrush", out object brush)
            && brush is Microsoft.UI.Xaml.Media.Brush background) root.Background = background;
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var bar = new CommandBar { DefaultLabelPosition = CommandBarDefaultLabelPosition.Right };
        var historyCommands = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        Button HistoryButton(string resource, Action action)
        {
            var button = new Button { Content = AppResources.Get(resource), VerticalAlignment = VerticalAlignment.Center };
            button.Click += (_, _) => action();
            historyCommands.Children.Add(button);
            return button;
        }
        _saveButton = HistoryButton("Panel_SaveDraft", () => TrySave());
        _undo = HistoryButton("Panel_Undo", () => ChangeHistory(false));
        _redo = HistoryButton("Panel_Redo", () => ChangeHistory(true));
        bar.Content = historyCommands;
        _edit = Command(bar, "Panel_EditRegion", Symbol.Edit, async () => await EditSelectedAsync());
        _merge = Command(bar, "Panel_Merge", Symbol.Link, async () => await MergeSelectedAsync());
        _delete = Command(bar, "Panel_DeleteRegions", Symbol.Delete, DeleteSelected);
        root.Children.Add(bar);

        var tools = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        _tools.Items.Add(AppResources.Get("Panel_SelectTool"));
        _tools.Items.Add(AppResources.Get("Panel_RectangleTool"));
        _tools.Items.Add(AppResources.Get("Panel_LineTool"));
        _tools.SelectedIndex = 0;
        AutomationProperties.SetName(_tools, AppResources.Get("Panel_Tools"));
        _lineScope.Items.Add(AppResources.Get("Panel_WholeBoard"));
        _lineScope.Items.Add(AppResources.Get("Panel_SelectedRegion"));
        _lineScope.SelectedIndex = 0;
        _lineScope.Visibility = Visibility.Collapsed;
        AutomationProperties.SetName(_lineScope, AppResources.Get("Panel_LineScope"));
        _tools.SelectionChanged += (_, _) =>
        {
            CancelDrag();
            SetHotEdge(null);
            _lineScope.Visibility = CurrentTool == Tool.Line ? Visibility.Visible : Visibility.Collapsed;
            ShowStatus(CurrentTool switch
            {
                Tool.Line => "Panel_LineHelp",
                Tool.Rectangle => "Panel_RectangleHelp",
                _ => "Panel_SelectHelp",
            });
        };
        tools.Children.Add(_tools);
        tools.Children.Add(_lineScope);
        Grid.SetRow(tools, 1);
        root.Children.Add(tools);
        _board.SetDeviceSize(width, height);
        _board.PointerPressed += Pressed;
        _board.PointerMoved += Moved;
        _board.PointerReleased += Released;
        _board.PointerCaptureLost += (_, _) => CancelDrag();
        _board.PointerExited += (_, _) => { if (_drag is null) SetHotEdge(null); };
        _board.RightTapped += RightTapped;
        _board.KeyDown += CanvasKeyDown;
        Grid.SetRow(_board, 2);
        root.Children.Add(_board);

        var dimensions = new TextBlock { Text = PanelBoard.DimensionsText(width, height), TextWrapping = TextWrapping.Wrap };
        Grid.SetRow(dimensions, 3);
        root.Children.Add(dimensions);
        AutomationProperties.SetLiveSetting(_status, AutomationLiveSetting.Polite);
        Grid.SetRow(_status, 4);
        root.Children.Add(_status);
        AutomationProperties.SetName(_regions, AppResources.Get("Panel_RegionList"));
        _regions.SelectionChanged += RegionSelectionChanged;
        _regions.KeyDown += CanvasKeyDown;
        Grid.SetRow(_regions, 5);
        root.Children.Add(_regions);
        root.KeyDown += (_, args) =>
        {
            if (args.Key is VirtualKey.Z or VirtualKey.Y) CanvasKeyDown(root, args);
        };
        Content = root;
        ShowStatus("Panel_SelectHelp");
        Refresh();
    }

    private Tool CurrentTool => (Tool)_tools.SelectedIndex;
    private string EmptyName => AppResources.Get("Panel_EmptyRegion");
    private PanelGeometry Geometry => _history.Current;

    public void BringForward()
    {
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
            presenter.Restore();
        Activate();
    }

    private static AppBarButton Command(CommandBar bar, string key, Symbol symbol, Action action)
    {
        var button = new AppBarButton { Label = AppResources.Get(key), Icon = new SymbolIcon(symbol) };
        button.Click += (_, _) => action();
        bar.PrimaryCommands.Add(button);
        return button;
    }

    public void SetRecording(bool recording)
    {
        _recording = recording;
        if (_dialogPickers is not null)
            foreach (KeyPicker picker in _dialogPickers) picker.CaptureEnabled = !recording;
        if (_recordingStatus is not null)
            _recordingStatus.Text = recording ? AppResources.Get("Panel_RecordingKey") : string.Empty;
        UpdateCommands();
    }

    private void ShowStatus(string? key)
    {
        string? message = key is null ? null : AppResources.Get(key);
        if (_status.Text == (message ?? string.Empty)) return;
        _status.Text = message ?? string.Empty;
        ToolTipService.SetToolTip(_status, message);
    }

    private void Pressed(object sender, PointerRoutedEventArgs args)
    {
        if (_recording || _dialogOpen || _drag is not null) return;
        var pointer = args.GetCurrentPoint(_board);
        bool touch = pointer.PointerDeviceType == PointerDeviceType.Touch;
        if (!touch && !pointer.Properties.IsLeftButtonPressed) return;
        PanelPoint point = _board.FromPoint(pointer.Position);
        if (!double.IsFinite(point.X) || !double.IsFinite(point.Y)
            || point.X < -.012 || point.X > 1.012 || point.Y < -.012 || point.Y > 1.012) return;
        point = CurrentTool == Tool.Select ? Clamp(point) : SnapToBoard(point);
        _board.Focus(FocusState.Pointer);
        args.Handled = true;
        bool control = (args.KeyModifiers & VirtualKeyModifiers.Control) != 0;
        PanelHit hit = _surface.Hit(point);
        PanelEdgeHandle? edge = null;
        if (CurrentTool == Tool.Select)
        {
            edge = !control ? _surface.FindEdge(point) : null;
            SetHotEdge(edge);
            if (edge is null)
            {
                if (!control) _selected.Clear();
                if (hit.RegionId is { } id && !_selected.Add(id) && control) _selected.Remove(id);
                RefreshSelection();
                if (control) return;
            }
        }
        _drag = new(point, hit.RegionId, edge, touch, Stopwatch.GetTimestamp());
        _board.CapturePointer(args.Pointer);
    }

    private void Moved(object sender, PointerRoutedEventArgs args)
    {
        PanelPoint point = _board.FromPoint(args.GetCurrentPoint(_board).Position);
        if (_drag is { } drag)
        {
            UpdateDrag(drag, point);
            return;
        }
        if (CurrentTool == Tool.Select)
        {
            SetHotEdge(_surface.FindEdge(point));
            PanelRegion? region = FindRegion(_surface.Hit(point).RegionId);
            string? tooltip = region is null ? null : AppResources.Format("Panel_RegionLabel", region.Name, PanelBoard.ActionLabel(region.Action));
            if (!Equals(ToolTipService.GetToolTip(_board), tooltip)) ToolTipService.SetToolTip(_board, tooltip);
        }
    }

    private void Released(object sender, PointerRoutedEventArgs args)
    {
        if (_drag is not { } drag) return;
        UpdateDrag(drag, _board.FromPoint(args.GetCurrentPoint(_board).Position));
        PanelEdit? edit = _preview;
        _drag = null;
        _preview = null;
        _board.ReleasePointerCapture(args.Pointer);
        if (edit is not null) Commit(edit);
        else _board.Render(Geometry, _selected, _hotEdge?.Edge);
        args.Handled = true;
    }

    private void UpdateDrag(Drag drag, PanelPoint point)
    {
        if (!double.IsFinite(point.X) || !double.IsFinite(point.Y)
            || new PanelEdge(drag.Start, point).Length < .004
            || drag.Touch && CurrentTool == Tool.Select && Stopwatch.GetElapsedTime(drag.Started) < TimeSpan.FromMilliseconds(420))
        {
            _preview = null;
            _board.Render(Geometry, _selected, _hotEdge?.Edge);
            return;
        }

        string? selected = SelectedOne();
        PanelEdge trace = new(drag.Start, SnapToBoard(point));
        PanelEdit? edit = CurrentTool switch
        {
            Tool.Line when _lineScope.SelectedIndex == 1 && selected is null => new(null, "Panel_SelectOne"),
            Tool.Line => PanelGeometryEditor.CutLine(Geometry, trace, _lineScope.SelectedIndex == 1 ? selected : null, EmptyName),
            Tool.Rectangle => PanelGeometryEditor.CutRectangle(Geometry, drag.Start, trace.End, selected, EmptyName),
            _ when drag.Edge is PanelSeam seam => PanelGeometryEditor.MoveEdge(Geometry, seam, point - drag.Start),
            _ when drag.Edge is PanelRectangleEdge edge => PanelGeometryEditor.ResizeRectangle(Geometry, edge, point - drag.Start),
            _ when drag.Region is { } region => PanelGeometryEditor.MoveRegion(Geometry, _surface, region, drag.Start, point, EmptyName),
            _ => null,
        };
        _preview = edit;
        ShowStatus(edit?.Error ?? (edit?.Changed == true && CurrentTool is Tool.Line or Tool.Rectangle
            ? "Panel_SplitPreview" : null));
        _board.Render(edit?.Geometry ?? Geometry, _selected, edit?.Changed == true ? null : drag.Edge?.Edge,
            CurrentTool == Tool.Line ? trace : null, edit?.Changed == true,
            CurrentTool == Tool.Rectangle && edit?.Error is not null ? trace : null);
    }

    private static PanelPoint Clamp(PanelPoint point) => new(Math.Clamp(point.X, 0, 1), Math.Clamp(point.Y, 0, 1));

    private static PanelPoint SnapToBoard(PanelPoint point)
    {
        static double Snap(double value) => value < .012 ? 0 : value > .988 ? 1 : value;
        return new(Snap(point.X), Snap(point.Y));
    }

    private void SetHotEdge(PanelEdgeHandle? edge)
    {
        if (edge == _hotEdge) return;
        _hotEdge = edge;
        _board.SetEdgeCursor(edge);
        _board.Render(Geometry, _selected, edge?.Edge);
    }

    private void CancelDrag()
    {
        if (_drag is null) return;
        _drag = null;
        _preview = null;
        _board.ReleasePointerCaptures();
        ShowStatus(null);
        _board.Render(Geometry, _selected, _hotEdge?.Edge);
    }

    private async void CanvasKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (_recording || _dialogOpen) return;
        bool control = (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control) & CoreVirtualKeyStates.Down) != 0;
        switch (args.Key)
        {
            case VirtualKey.Escape when _drag is not null: CancelDrag(); break;
            case VirtualKey.Z when control: ChangeHistory(false); break;
            case VirtualKey.Y when control: ChangeHistory(true); break;
            case VirtualKey.Delete: DeleteSelected(); break;
            case VirtualKey.Enter when SelectedOne() is not null:
                args.Handled = true;
                await EditSelectedAsync();
                break;
            default: return;
        }
        args.Handled = true;
    }

    private void RightTapped(object sender, RightTappedRoutedEventArgs args)
    {
        string? id = _surface.Hit(_board.FromPoint(args.GetPosition(_board))).RegionId;
        if (id is null || _recording) return;
        if (!_selected.Contains(id))
        {
            _selected.Clear();
            _selected.Add(id);
            RefreshSelection();
        }
        var menu = new MenuFlyout();
        void Add(string text, Action action)
        {
            var item = new MenuFlyoutItem { Text = AppResources.Get(text) };
            item.Click += (_, _) => action();
            menu.Items.Add(item);
        }
        if (_selected.Count == 1) Add("Panel_EditRegion", async () => await EditSelectedAsync());
        if (_selected.Count > 1) Add("Panel_Merge", async () => await MergeSelectedAsync());
        Add("Panel_DeleteRegions", DeleteSelected);
        menu.ShowAt(_board, new FlyoutShowOptions { Position = args.GetPosition(_board) });
        args.Handled = true;
    }

    private void RegionSelectionChanged(object sender, SelectionChangedEventArgs args)
    {
        if (_updatingRegionList) return;
        _selected.Clear();
        foreach (ListViewItem item in _regions.SelectedItems.OfType<ListViewItem>())
            if (item.Tag is string id) _selected.Add(id);
        _board.Render(Geometry, _selected);
        UpdateCommands();
    }

    private string? SelectedOne() => _selected.Count == 1 ? _selected.First() : null;

    private PanelRegion? FindRegion(string? id) => Geometry.Regions.FirstOrDefault(region => region.Id == id);

    private void Refresh()
    {
        _surface = new(Geometry);
        _board.SetEdgeCursor(_hotEdge);
        _selected.IntersectWith(Geometry.Regions.Select(region => region.Id));
        _updatingRegionList = true;
        _regions.Items.Clear();
        foreach (PanelRegion region in Geometry.Regions)
        {
            var item = new ListViewItem
            {
                Tag = region.Id,
                Content = AppResources.Format("Panel_RegionLabel", region.Name, PanelBoard.ActionLabel(region.Action)),
            };
            item.GotFocus += (_, _) =>
            {
                if (!_selected.Contains(region.Id)) _board.Render(Geometry, new HashSet<string> { region.Id });
            };
            item.LostFocus += (_, _) => _board.Render(Geometry, _selected);
            _regions.Items.Add(item);
        }
        _updatingRegionList = false;
        RefreshSelection();
    }

    private void RefreshSelection()
    {
        _updatingRegionList = true;
        _regions.SelectedItems.Clear();
        foreach (ListViewItem item in _regions.Items.OfType<ListViewItem>())
            if (item.Tag is string id && _selected.Contains(id)) _regions.SelectedItems.Add(item);
        _updatingRegionList = false;
        _board.Render(Geometry, _selected, _hotEdge?.Edge);
        UpdateCommands();
    }

    private void UpdateCommands()
    {
        _saveButton.IsEnabled = !_recording && !_dialogOpen;
        _undo.IsEnabled = _history.CanUndo && !_recording;
        _redo.IsEnabled = _history.CanRedo && !_recording;
        _edit.IsEnabled = _selected.Count == 1 && !_recording;
        _merge.IsEnabled = _selected.Count > 1 && !_recording;
        _delete.IsEnabled = _selected.Count > 0 && !_recording;
    }

    private void Commit(PanelEdit edit)
    {
        ShowStatus(edit.Error);
        _history.Commit(edit);
        if (!edit.Changed) return;
        _hotEdge = null;
        Refresh();
    }

    private void ChangeHistory(bool redo)
    {
        CancelDrag();
        if (redo) _history.Redo(); else _history.Undo();
        ShowStatus(null);
        _hotEdge = null;
        Refresh();
    }

    private void DeleteSelected()
    {
        if (_recording || _dialogOpen || _selected.Count == 0) return;
        CancelDrag();
        Commit(PanelGeometryEditor.Delete(Geometry, _selected));
    }

    private async Task MergeSelectedAsync()
    {
        if (_recording || _dialogOpen || _selected.Count < 2) return;
        PanelRegion[] selected = Geometry.Regions.Where(region => _selected.Contains(region.Id)).ToArray();
        string inherit = selected[0].Id;
        if (selected.Any(region => !PanelGeometryEditor.ActionsEqual(region.Action, selected[0].Action)))
        {
            var choice = new ComboBox { Header = AppResources.Get("Panel_MergeInherit"), MinWidth = 220 };
            foreach (PanelRegion region in selected)
                choice.Items.Add(new ComboBoxItem { Tag = region.Id, Content = AppResources.Format("Panel_RegionLabel", region.Name, PanelBoard.ActionLabel(region.Action)) });
            choice.SelectedIndex = 0;
            var dialog = Dialog("Panel_Merge", choice, "Panel_Merge");
            if (await ShowDialogAsync(dialog) != ContentDialogResult.Primary || choice.SelectedItem is not ComboBoxItem { Tag: string id }) return;
            inherit = id;
        }
        Commit(PanelGeometryEditor.Merge(Geometry, _selected, inherit));
    }

    private async Task EditSelectedAsync()
    {
        if (_recording || _dialogOpen || FindRegion(SelectedOne()) is not { } region) return;
        var name = new TextBox { Header = AppResources.Get("Panel_Name"), Text = region.Name, MaxLength = 80 };
        var kind = new ComboBox { Header = AppResources.Get("Panel_Action") };
        foreach (string key in new[] { "Presentation_Unbound", "Panel_ActionKey", "Panel_ActionDigit", "Panel_ActionOperator" })
            kind.Items.Add(AppResources.Get(key));
        var digit = new ComboBox { Header = AppResources.Get("Panel_ActionDigit") };
        for (int i = 0; i <= 9; i++) digit.Items.Add(i);
        digit.SelectedIndex = region.Action is PanelDigit d ? d.Digit : 0;
        var arithmetic = new ComboBox { Header = AppResources.Get("Panel_ActionOperator") };
        foreach (PanelOperator op in Enum.GetValues<PanelOperator>()) arithmetic.Items.Add(PanelBoard.OperatorSymbol(op));
        arithmetic.SelectedIndex = region.Action is PanelArithmetic a ? (int)a.Operator : 0;
        var pickers = new StackPanel { Spacing = 4 };
        var keyArea = new StackPanel { Spacing = 4 };
        keyArea.Children.Add(pickers);
        List<KeyPicker> keys = [];
        Button add = new() { Content = AppResources.Get("Panel_AddKey") };
        var recordingStatus = new TextBlock { TextWrapping = TextWrapping.Wrap };
        AutomationProperties.SetLiveSetting(recordingStatus, AutomationLiveSetting.Polite);
        keyArea.Children.Add(add);
        keyArea.Children.Add(recordingStatus);
        _dialogPickers = keys;
        _recordingStatus = recordingStatus;
        void AddPicker(KeyIdentity identity)
        {
            var picker = new KeyPicker();
            picker.SetIdentity(identity);
            keys.Add(picker);
            AutomationProperties.SetName(picker, AppResources.Format("Panel_KeyNumber", keys.Count));
            picker.CaptureRequested += async (_, _) => await _capture(picker);
            var row = new StackPanel { Spacing = 3 };
            row.Children.Add(picker);
            var remove = new Button { Content = AppResources.Get("Panel_RemoveKey") };
            remove.Click += (_, _) =>
            {
                if (_recording || keys.Count == 1) return;
                keys.Remove(picker);
                pickers.Children.Remove(row);
                add.IsEnabled = true;
            };
            row.Children.Add(remove);
            pickers.Children.Add(row);
            add.IsEnabled = keys.Count < 8;
        }
        foreach (KeyIdentity key in (region.Action as PanelKeys)?.Keys ?? [KeyIdentity.Logical("A")]) AddPicker(key);
        add.Click += (_, _) => { if (!_recording && keys.Count < 8) AddPicker(KeyIdentity.Logical("A")); };
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var panel = new StackPanel { Spacing = 7 };
        foreach (UIElement element in new UIElement[] { name, kind, keyArea, digit, arithmetic, error }) panel.Children.Add(element);
        void ShowFields()
        {
            keyArea.Visibility = kind.SelectedIndex == 1 ? Visibility.Visible : Visibility.Collapsed;
            digit.Visibility = kind.SelectedIndex == 2 ? Visibility.Visible : Visibility.Collapsed;
            arithmetic.Visibility = kind.SelectedIndex == 3 ? Visibility.Visible : Visibility.Collapsed;
        }
        kind.SelectedIndex = region.Action switch { PanelKeys => 1, PanelDigit => 2, PanelArithmetic => 3, _ => 0 };
        kind.SelectionChanged += (_, _) => ShowFields();
        ShowFields();
        var scroll = new ScrollViewer { Content = panel, MaxHeight = 400, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        var dialog = Dialog("Panel_EditRegion", scroll, "Panel_Done");
        PanelEdit? edit = null;
        dialog.PrimaryButtonClick += (_, args) =>
        {
            if (_recording) { args.Cancel = true; return; }
            List<KeyIdentity> chosen = [];
            if (kind.SelectedIndex == 1)
            {
                foreach (KeyPicker picker in keys)
                {
                    if (!picker.TryGetIdentity(out KeyIdentity? key, out string? reason))
                    {
                        error.Text = reason;
                        args.Cancel = true;
                        return;
                    }
                    chosen.Add(key!);
                }
            }
            PanelAction? action = kind.SelectedIndex switch
            {
                1 => new PanelKeys(chosen),
                2 => new PanelDigit(digit.SelectedIndex),
                3 => new PanelArithmetic((PanelOperator)arithmetic.SelectedIndex),
                _ => null,
            };
            edit = PanelGeometryEditor.Replace(Geometry, region with { Name = name.Text.Trim(), Action = action });
            if (edit.Error is { } validation)
            {
                error.Text = AppResources.Get(validation);
                args.Cancel = true;
            }
        };
        try
        {
            if (await ShowDialogAsync(dialog) == ContentDialogResult.Primary && edit is not null) Commit(edit);
        }
        finally
        {
            if (_recording) await _cancelCapture();
            _dialogPickers = null;
            _recordingStatus = null;
        }
    }

    private ContentDialog Dialog(string title, object content, string primary) => new()
    {
        XamlRoot = _board.XamlRoot, Title = AppResources.Get(title), Content = content,
        PrimaryButtonText = AppResources.Get(primary), CloseButtonText = AppResources.Get("Common_Back"),
        DefaultButton = ContentDialogButton.Primary,
    };

    private async Task<ContentDialogResult> ShowDialogAsync(ContentDialog dialog)
    {
        _dialogOpen = true;
        try { return await dialog.ShowAsync(); }
        finally { _dialogOpen = false; }
    }

    private bool TrySave()
    {
        if (_save(Geometry) is { } error)
        {
            ShowStatus(error);
            return false;
        }
        CloseImmediately();
        return true;
    }

    private async void WindowClosing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        if (_allowClose) return;
        args.Cancel = true;
        await ConfirmCloseAsync();
    }

    public async Task<bool> ConfirmCloseAsync()
    {
        if (_closing || _dialogOpen) { BringForward(); return false; }
        if (!_history.IsDirty) { CloseImmediately(); return true; }
        _closing = true;
        try
        {
            BringForward();
            var dialog = Dialog("Panel_CloseTitle", AppResources.Get("Panel_CloseMessage"), "Panel_SaveDraft");
            dialog.SecondaryButtonText = AppResources.Get("Panel_Discard");
            dialog.CloseButtonText = AppResources.Get("Panel_Continue");
            switch (await ShowDialogAsync(dialog))
            {
                case ContentDialogResult.Primary: return TrySave();
                case ContentDialogResult.Secondary: CloseImmediately(); return true;
                default: return false;
            }
        }
        finally { _closing = false; }
    }

    private void CloseImmediately()
    {
        _allowClose = true;
        Close();
    }
}
