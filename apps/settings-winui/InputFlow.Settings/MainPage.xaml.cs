using System.Diagnostics;
using System.Reflection;
using System.Runtime.InteropServices;
using InputFlow.Settings.Core;
using InputFlow_Settings.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Navigation;
using Windows.System;

namespace InputFlow_Settings;

public sealed partial class MainPage : Page
{
    private readonly DraftSession _draft = new();
    private readonly List<KeyPicker> _actionPickers = [];
    private readonly CancellationTokenSource _pageLifetime = new();
    private readonly CaptureUiIntentTracker _captureIntent = new();
    private AgentConnectionCoordinator? _coordinator;
    private ConfigSaveService? _saveService;
    private string? _editingOriginalId;
    private bool _refreshingRules;
    private bool _busy;
    private KeyPicker? _captureKeyTarget;
    private bool _captureMouseTarget;
    private CancellationTokenSource? _captureCountdown;
    private DispatcherTimer? _directionPreviewTimer;
    private Stopwatch? _directionPreviewClock;
    private CursorPoint _directionPreviewOrigin;

    [StructLayout(LayoutKind.Sequential)]
    private struct CursorPoint
    {
        public int X;
        public int Y;
    }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetCursorPos(out CursorPoint point);

    public MainPage()
    {
        InitializeComponent();
        MouseButtonCombo.ItemsSource = InputCatalog.MouseButtons;
        MouseButtonCombo.SelectedIndex = 1;
        DirectionCombo.ItemsSource = InputCatalog.MouseDirections;
        DirectionCombo.SelectedItem = MouseDirection.Right;
        TriggerTypeCombo.SelectedIndex = 0;
        Version version = Assembly.GetExecutingAssembly().GetName().Version ?? new Version(0, 1, 0);
        VersionText.Text = $"版本 {version.ToString(3)}　进程架构 {RuntimeInformation.ProcessArchitecture}　配置 Schema v{ConfigDocument.CurrentSchemaVersion}";
        _draft.Changed += Draft_Changed;
        SizeChanged += MainPage_SizeChanged;
    }

    public bool HasUnsavedChanges => _draft.IsDirty || RuleEditor.Visibility == Visibility.Visible;

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        _coordinator = (AgentConnectionCoordinator)e.Parameter;
        _saveService = new ConfigSaveService(_coordinator);
        _coordinator.ConnectionChanged += Coordinator_ConnectionChanged;
        _coordinator.AuthorityChanged += Coordinator_AuthorityChanged;
        _coordinator.CaptureCompleted += Coordinator_CaptureCompleted;
    }

    public async Task<bool> PrepareToCloseAsync()
    {
        await CancelCaptureBestEffortAsync().ConfigureAwait(true);
        if (!HasUnsavedChanges)
        {
            return true;
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "有未保存的草稿",
            Content = "当前规则编辑或草稿只存在于设置程序内存中，尚未由 Agent 保存。保存成功后退出，或明确放弃这些更改。",
            PrimaryButtonText = "保存并退出",
            SecondaryButtonText = "不保存退出",
            CloseButtonText = "返回",
            DefaultButton = ContentDialogButton.Primary,
        };
        ContentDialogResult choice = await dialog.ShowAsync();
        if (choice == ContentDialogResult.Secondary)
        {
            return true;
        }

        if (choice != ContentDialogResult.Primary)
        {
            return false;
        }

        if (RuleEditor.Visibility == Visibility.Visible)
        {
            if (!TryBuildEditedRule(out RuleDocument? rule, out string? error))
            {
                ShowEditorError(error ?? "当前规则编辑无法写入草稿。");
                return false;
            }

            _draft.UpsertRule(rule!, _editingOriginalId);
            RuleEditor.Visibility = Visibility.Collapsed;
            UpdateResponsiveEditor();
        }

        SaveResult? result = await SaveDraftAsync(overwriteExternalChanges: false);
        return result?.IsApplied == true;
    }

    private async void Page_Loaded(object sender, RoutedEventArgs e)
    {
        if (_coordinator is null)
        {
            return;
        }

        try
        {
            await _coordinator.ConnectAsync(_pageLifetime.Token);
        }
        catch (Exception error)
        {
            ShowMessage("Agent 未连接", error.Message, InfoBarSeverity.Error);
        }
    }

    private void Page_Unloaded(object sender, RoutedEventArgs e)
    {
        StopDirectionPreview("预览已停止。", resetProgress: false);
        _pageLifetime.Cancel();
        _captureCountdown?.Cancel();
        if (_coordinator is not null)
        {
            _coordinator.ConnectionChanged -= Coordinator_ConnectionChanged;
            _coordinator.AuthorityChanged -= Coordinator_AuthorityChanged;
            _coordinator.CaptureCompleted -= Coordinator_CaptureCompleted;
        }
    }

    private void Coordinator_ConnectionChanged(object? sender, AgentConnectionSnapshot snapshot) =>
        DispatcherQueue.TryEnqueue(() => UpdateConnection(snapshot));

    private void Coordinator_AuthorityChanged(object? sender, EventArgs e) =>
        DispatcherQueue.TryEnqueue(ApplyAuthoritySnapshot);

    private void Coordinator_CaptureCompleted(object? sender, CaptureTerminal terminal) =>
        DispatcherQueue.TryEnqueue(() => HandleCaptureTerminal(terminal));

    private void UpdateConnection(AgentConnectionSnapshot snapshot)
    {
        if (!snapshot.ControlConnected && _directionPreviewTimer is not null)
        {
            StopDirectionPreview("Agent 连接已断开；方向预览已取消。", resetProgress: false);
        }
        ConnectionBar.IsOpen = true;
        ConnectionBar.Title = snapshot.Kind == AgentConnectionKind.Online ? "Agent 已连接" : "Agent 连接状态";
        ConnectionBar.Message = snapshot.Message;
        ConnectionBar.Severity = snapshot.Kind switch
        {
            AgentConnectionKind.Online => InfoBarSeverity.Success,
            AgentConnectionKind.Connecting or AgentConnectionKind.EventStreamReconnecting => InfoBarSeverity.Warning,
            _ => InfoBarSeverity.Error,
        };
        SaveButton.IsEnabled = snapshot.ControlConnected && !_busy;
        PauseButton.IsEnabled = snapshot.ControlConnected && !_busy;
    }

    private void ApplyAuthoritySnapshot()
    {
        if (_coordinator?.LatestConfig is ConfigDocument formal)
        {
            if (!_draft.IsLoaded)
            {
                _draft.Load(formal);
                EmergencyKeyPicker.SetIdentity(formal.EmergencyBypassKey);
            }
            else if (!_draft.IsDirty && !ConfigCodec.DeepEquals(_draft.FormalSnapshot, formal))
            {
                _draft.Load(formal);
                EmergencyKeyPicker.SetIdentity(formal.EmergencyBypassKey);
            }
            else if (_draft.IsDirty && !ConfigCodec.DeepEquals(_draft.FormalSnapshot, formal))
            {
                ShowMessage(
                    "正式配置已变化",
                    "另一客户端已经更改规则。保存时会再次比较，并要求重新加载或确认覆盖。",
                    InfoBarSeverity.Warning);
            }
        }

        if (_coordinator?.LatestStatus is AgentStatus status)
        {
            string suspension = status.Suspended ? "已暂停" : "运行中";
            string statusSummary = $"phase={status.Phase}　{suspension}　启用规则={status.RuleCount}　reconciliation={status.ApplyReconciliation}";
            AgentStatusText.Text = statusSummary;
            AgentErrorText.Text = status.LastError ?? string.Empty;
            PauseButton.Label = status.Suspended ? "恢复" : "暂停";
            PauseButton.Icon = new SymbolIcon(status.Suspended ? Symbol.Play : Symbol.Pause);
            if (_coordinator.Connection.Kind == AgentConnectionKind.Online)
            {
                ConnectionBar.IsOpen = true;
                ConnectionBar.Title = "Agent 已连接";
                ConnectionBar.Message = status.LastError is null
                    ? statusSummary
                    : $"{statusSummary}　最近错误：{status.LastError}";
                ConnectionBar.Severity = status.ApplyReconciliation == "recovery_required" || status.LastError is not null
                    ? InfoBarSeverity.Warning
                    : InfoBarSeverity.Success;
            }
        }
    }

    private void Draft_Changed(object? sender, EventArgs e)
    {
        DispatcherQueue.TryEnqueue(() =>
        {
            DirtyText.Visibility = _draft.IsDirty ? Visibility.Visible : Visibility.Collapsed;
            RefreshRuleList();
            if (_draft.IsLoaded)
            {
                EmergencyKeyPicker.SetIdentity(_draft.Draft.EmergencyBypassKey);
            }
        });
    }

    private async void RetryConnection_Click(object sender, RoutedEventArgs e)
    {
        if (_coordinator is null || _busy) return;
        SetBusy(true);
        try
        {
            await _coordinator.ReconnectAsync(_pageLifetime.Token);
        }
        catch (Exception error)
        {
            ShowMessage("重连失败", error.Message, InfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private async void Reload_Click(object sender, RoutedEventArgs e)
    {
        if (_coordinator is null || _busy) return;
        if (_draft.IsDirty)
        {
            var dialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "放弃未保存草稿？",
                Content = "重新加载会丢弃当前设置程序中的未保存更改，不会修改 Agent。",
                PrimaryButtonText = "放弃并重新加载",
                CloseButtonText = "取消",
                DefaultButton = ContentDialogButton.Close,
            };
            if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        }

        SetBusy(true);
        try
        {
            ConfigDocument formal = await _coordinator.GetConfigAsync(_pageLifetime.Token);
            await _coordinator.GetStatusAsync(_pageLifetime.Token);
            _draft.Load(formal);
            ShowMessage("已重新加载", "草稿已重置为 Agent 的正式配置。", InfoBarSeverity.Success);
        }
        catch (Exception error)
        {
            ShowMessage("重新加载失败", error.Message, InfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private async void Save_Click(object sender, RoutedEventArgs e) =>
        await SaveDraftAsync(overwriteExternalChanges: false);

    private async Task<SaveResult?> SaveDraftAsync(bool overwriteExternalChanges)
    {
        if (_saveService is null || !_draft.IsLoaded || _busy) return null;
        if (RuleEditor.Visibility == Visibility.Visible)
        {
            ShowEditorError("请先“写入草稿”或取消当前规则编辑，再保存整份配置。");
            return null;
        }

        SetBusy(true);
        try
        {
            SaveResult result = await _saveService.SaveAsync(
                _draft.Draft,
                _draft.FormalSnapshot,
                overwriteExternalChanges,
                _pageLifetime.Token);
            if (result.Kind == SaveResultKind.ExternalChangeDetected)
            {
                var dialog = new ContentDialog
                {
                    XamlRoot = XamlRoot,
                    Title = "正式规则已在其他地方改变",
                    Content = result.Message,
                    PrimaryButtonText = "确认覆盖",
                    SecondaryButtonText = "重新加载",
                    CloseButtonText = "保留草稿",
                    DefaultButton = ContentDialogButton.Close,
                };
                ContentDialogResult decision = await dialog.ShowAsync();
                SetBusy(false);
                if (decision == ContentDialogResult.Primary)
                {
                    return await SaveDraftAsync(overwriteExternalChanges: true);
                }

                if (decision == ContentDialogResult.Secondary)
                {
                    ConfigDocument formal = await _coordinator!.GetConfigAsync(_pageLifetime.Token);
                    _draft.Load(formal);
                }

                return result;
            }

            if (result.IsApplied && result.ConfirmedFormal is not null)
            {
                _draft.AcceptSaved(result.ConfirmedFormal);
            }

            string details = result.ValidationErrors.Count == 0
                ? result.Message
                : $"{result.Message}\n• {string.Join("\n• ", result.ValidationErrors)}";
            if (result.CleanupWarnings.Count > 0)
            {
                details += $"\n清理警告：{string.Join("；", result.CleanupWarnings)}";
            }

            ShowMessage(
                result.IsApplied ? "保存完成" : SaveTitle(result.Kind),
                details,
                result.IsApplied ? InfoBarSeverity.Success : SaveSeverity(result.Kind));
            return result;
        }
        catch (Exception error)
        {
            ShowMessage("保存未完成", $"{error.Message}。若请求已经发出，超时不等于保存失败，请重连核对。", InfoBarSeverity.Error);
            return null;
        }
        finally
        {
            SetBusy(false);
        }
    }

    private async void PauseResume_Click(object sender, RoutedEventArgs e)
    {
        if (_coordinator?.LatestStatus is not AgentStatus status || _busy) return;
        SetBusy(true);
        try
        {
            AgentStatus confirmed = status.Suspended
                ? await _coordinator.ResumeAsync(_pageLifetime.Token)
                : await _coordinator.PauseAsync(_pageLifetime.Token);
            ShowMessage(
                confirmed.Suspended ? "Agent 已暂停" : "Agent 已恢复",
                "状态已由 Agent 确认；托盘、F12 和其他客户端的后续变化会通过事件同步。",
                InfoBarSeverity.Success);
        }
        catch (Exception error)
        {
            ShowMessage("运行控制失败", error.Message, InfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private async void RefreshStats_Click(object sender, RoutedEventArgs e)
    {
        if (_coordinator is null || _busy) return;
        SetBusy(true);
        try
        {
            AgentStats stats = await _coordinator.GetStatsAsync(_pageLifetime.Token);
            StatsText.Text = $"observed_events: {stats.ObservedEvents}\n" +
                $"output batches: sent={stats.OutputBatchesSent}, failed={stats.OutputBatchesFailed}, dropped={stats.OutputBatchesDropped}\n" +
                $"callback_latency_us: {FormatPercentiles(stats.CallbackLatencyMicroseconds)}\n" +
                $"hold_delay_us: {FormatPercentiles(stats.HoldDelayMicroseconds)}";
        }
        catch (Exception error)
        {
            StatsText.Text = $"查询失败：{error.Message}";
        }
        finally
        {
            SetBusy(false);
        }
    }

    private void RootNavigation_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        string tag = (args.SelectedItemContainer?.Tag as string) ?? "rules";
        RulesPage.Visibility = tag == "rules" ? Visibility.Visible : Visibility.Collapsed;
        SettingsPage.Visibility = tag == "settings" ? Visibility.Visible : Visibility.Collapsed;
        AboutPage.Visibility = tag == "about" ? Visibility.Visible : Visibility.Collapsed;
    }

    private void AddRule_Click(object sender, RoutedEventArgs e)
    {
        if (!_draft.IsLoaded) return;
        _editingOriginalId = null;
        EditorTitle.Text = "新建规则";
        RuleIdBox.Text = $"rule-{Guid.NewGuid():N}"[..13];
        EditorEnabled.IsOn = true;
        TriggerTypeCombo.SelectedIndex = 0;
        PrimaryKeyPicker.SetIdentity(KeyIdentity.Logical("A"));
        SecondaryKeyPicker.SetIdentity(KeyIdentity.Logical("B"));
        MouseButtonCombo.SelectedItem = "Right";
        TimeoutBox.Value = 250;
        DirectionCombo.SelectedItem = MouseDirection.Right;
        DirectionDistanceBox.Value = 80;
        DirectionDurationBox.Value = 500;
        DirectionToleranceBox.Value = 40;
        ClearActionPickers();
        AddActionPicker(KeyIdentity.Logical("LeftCtrl"));
        AddActionPicker(KeyIdentity.Logical("C"));
        EditorInfo.IsOpen = false;
        RuleEditor.Visibility = Visibility.Visible;
        UpdateResponsiveEditor();
        RuleIdBox.Focus(FocusState.Programmatic);
    }

    private void EditRule_Click(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not string id || !_draft.IsLoaded) return;
        RuleDocument? rule = _draft.Draft.Rules.FirstOrDefault(candidate => candidate.Id == id);
        if (rule is null) return;
        _editingOriginalId = rule.Id;
        EditorTitle.Text = "编辑规则";
        RuleIdBox.Text = rule.Id;
        EditorEnabled.IsOn = rule.Enabled;
        SelectTriggerType(rule.Trigger.Type);
        PrimaryKeyPicker.SetIdentity(rule.Trigger.FirstKey);
        switch (rule.Trigger)
        {
            case KeyChordTrigger chord:
                SecondaryKeyPicker.SetIdentity(chord.Second);
                break;
            case KeyMouseButtonTrigger mouse:
                MouseButtonCombo.SelectedItem = mouse.Button;
                break;
            case HoldTrigger hold:
                TimeoutBox.Value = hold.TimeoutMilliseconds;
                break;
            case HoldMouseButtonTrigger holdMouse:
                TimeoutBox.Value = holdMouse.TimeoutMilliseconds;
                MouseButtonCombo.SelectedItem = holdMouse.Button;
                break;
            case MouseDirectionTrigger direction:
                DirectionCombo.SelectedItem = direction.Direction;
                DirectionDistanceBox.Value = direction.MinimumDistancePixels;
                DirectionDurationBox.Value = direction.MaximumDurationMilliseconds;
                DirectionToleranceBox.Value = direction.OffAxisTolerancePixels;
                break;
        }

        ClearActionPickers();
        foreach (KeyIdentity key in rule.Action.Keys) AddActionPicker(key);
        EditorInfo.IsOpen = false;
        RuleEditor.Visibility = Visibility.Visible;
        UpdateResponsiveEditor();
        RuleIdBox.Focus(FocusState.Programmatic);
    }

    private async void DeleteRule_Click(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not string id) return;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = $"从草稿删除 {id}？",
            Content = "删除只修改本地草稿；点击“验证并保存”后才会影响 Agent。",
            PrimaryButtonText = "删除",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) _draft.RemoveRule(id);
    }

    private void TriggerTypeCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        string type = SelectedTriggerType();
        SecondKeyPanel.Visibility = type == "key_chord" ? Visibility.Visible : Visibility.Collapsed;
        MousePanel.Visibility = type is "key_mouse_button" or "hold_mouse_button" ? Visibility.Visible : Visibility.Collapsed;
        TimeoutPanel.Visibility = type is "hold" or "hold_mouse_button" ? Visibility.Visible : Visibility.Collapsed;
        DirectionPanel.Visibility = type == "mouse_direction" ? Visibility.Visible : Visibility.Collapsed;
        if (type != "mouse_direction") StopDirectionPreview("预览不保存轨迹，也不执行动作；点击开始相当于临时激活。", resetProgress: true);
        TimeoutHelpText.Text = type == "hold_mouse_button"
            ? "计时对象是上方的键盘键；达到阈值后再按鼠标按钮。鼠标按钮本身不需要长按。"
            : "计时对象是上方的键盘键；达到阈值后触发动作。";
    }

    private void CommitEdit_Click(object sender, RoutedEventArgs e)
    {
        if (!TryBuildEditedRule(out RuleDocument? rule, out string? error))
        {
            ShowEditorError(error ?? "规则字段不完整。");
            return;
        }

        StopDirectionPreview("预览已停止。", resetProgress: false);
        _draft.UpsertRule(rule!, _editingOriginalId);
        RuleEditor.Visibility = Visibility.Collapsed;
        UpdateResponsiveEditor();
    }

    private void CancelEdit_Click(object sender, RoutedEventArgs e)
    {
        StopDirectionPreview("预览已取消。", resetProgress: false);
        RuleEditor.Visibility = Visibility.Collapsed;
        EditorInfo.IsOpen = false;
        _editingOriginalId = null;
        UpdateResponsiveEditor();
    }

    private bool TryBuildEditedRule(out RuleDocument? rule, out string? error)
    {
        rule = null;
        string id = RuleIdBox.Text.Trim();
        if (id.Length == 0)
        {
            error = "规则 ID 不能为空。";
            return false;
        }

        if (!PrimaryKeyPicker.TryGetIdentity(out KeyIdentity? first, out error)) return false;
        string type = SelectedTriggerType();
        RuleTrigger trigger;
        if (type == "key_chord")
        {
            if (!SecondaryKeyPicker.TryGetIdentity(out KeyIdentity? second, out error)) return false;
            trigger = new KeyChordTrigger(first!, second!);
        }
        else if (type == "key_mouse_button")
        {
            trigger = new KeyMouseButtonTrigger(first!, MouseButtonCombo.SelectedItem as string ?? "Right");
        }
        else if (type is "hold" or "hold_mouse_button")
        {
            if (double.IsNaN(TimeoutBox.Value) || TimeoutBox.Value is < 1 or > 60000)
            {
                error = "timeout_ms 必须在 1–60000 之间。";
                return false;
            }

            ulong timeout = (ulong)TimeoutBox.Value;
            trigger = type == "hold"
                ? new HoldTrigger(first!, timeout)
                : new HoldMouseButtonTrigger(first!, timeout, MouseButtonCombo.SelectedItem as string ?? "Right");
        }
        else
        {
            if (DirectionCombo.SelectedItem is not MouseDirection direction)
            {
                error = "请选择鼠标方向。";
                return false;
            }
            if (!TryDirectionParameters(out uint distance, out ulong duration, out uint tolerance, out error))
            {
                return false;
            }
            trigger = new MouseDirectionTrigger(first!, direction, distance, duration, tolerance);
        }

        var actionKeys = new List<KeyIdentity>();
        foreach (KeyPicker picker in _actionPickers)
        {
            if (!picker.TryGetIdentity(out KeyIdentity? key, out error)) return false;
            actionKeys.Add(key!);
        }

        if (actionKeys.Count == 0)
        {
            error = "动作至少需要一个键。";
            return false;
        }

        rule = new RuleDocument
        {
            Id = id,
            Enabled = EditorEnabled.IsOn,
            Trigger = trigger,
            Action = new KeyChordAction(actionKeys),
        };
        error = null;
        return true;
    }

    private bool TryDirectionParameters(
        out uint distance,
        out ulong duration,
        out uint tolerance,
        out string? error)
    {
        distance = 0;
        duration = 0;
        tolerance = 0;
        double distanceValue = DirectionDistanceBox.Value;
        double durationValue = DirectionDurationBox.Value;
        double toleranceValue = DirectionToleranceBox.Value;
        if (double.IsNaN(distanceValue) || distanceValue is < 10 or > 2000 || distanceValue != Math.Truncate(distanceValue))
        {
            error = "最小净位移必须是 10–2000 的整数屏幕像素。";
            return false;
        }
        if (double.IsNaN(durationValue) || durationValue is < 100 or > 5000 || durationValue != Math.Truncate(durationValue))
        {
            error = "最大时间窗必须是 100–5000 的整数毫秒。";
            return false;
        }
        if (double.IsNaN(toleranceValue) || toleranceValue is < 0 or > 2000 || toleranceValue != Math.Truncate(toleranceValue))
        {
            error = "偏轴容差必须是 0–2000 的整数屏幕像素。";
            return false;
        }
        distance = (uint)distanceValue;
        duration = (ulong)durationValue;
        tolerance = (uint)toleranceValue;
        error = null;
        return true;
    }

    private void StartDirectionPreview_Click(object sender, RoutedEventArgs e)
    {
        if (_captureIntent.IsInProgress)
        {
            ShowEditorError("输入录制进行中；请先取消录制再开始方向预览。");
            return;
        }
        if (DirectionCombo.SelectedItem is not MouseDirection expected)
        {
            ShowEditorError("请选择鼠标方向。");
            return;
        }
        if (!TryDirectionParameters(out uint distance, out ulong duration, out uint tolerance, out string? error))
        {
            ShowEditorError(error ?? "方向预览参数不完整。");
            return;
        }
        if (!GetCursorPos(out _directionPreviewOrigin))
        {
            ShowEditorError("无法读取当前光标屏幕坐标，方向预览没有开始。");
            return;
        }

        StopDirectionPreview("", resetProgress: true);
        EditorInfo.IsOpen = false;
        _directionPreviewClock = Stopwatch.StartNew();
        _directionPreviewTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(33) };
        _directionPreviewTimer.Tick += (_, _) => UpdateDirectionPreview(expected, distance, duration, tolerance);
        _directionPreviewTimer.Start();
        StartDirectionPreviewButton.IsEnabled = false;
        CancelDirectionPreviewButton.IsEnabled = true;
        DirectionPreviewText.Text = $"预览已开始：起点 ({_directionPreviewOrigin.X}, {_directionPreviewOrigin.Y})。无需按激活键；本按钮只替代本次临时激活，最多 {duration} ms。";
    }

    private void CancelDirectionPreview_Click(object sender, RoutedEventArgs e) =>
        StopDirectionPreview("预览已取消；未保存坐标轨迹，也未执行动作。", resetProgress: false);

    private void UpdateDirectionPreview(MouseDirection expected, uint distance, ulong duration, uint tolerance)
    {
        if (_directionPreviewClock is null || _directionPreviewClock.ElapsedMilliseconds > (long)duration)
        {
            StopDirectionPreview("预览时间窗已结束；未命中且未执行动作。", resetProgress: false);
            return;
        }
        if (!GetCursorPos(out CursorPoint current))
        {
            StopDirectionPreview("读取光标位置失败；预览已安全停止。", resetProgress: false);
            return;
        }

        long dx = (long)current.X - _directionPreviewOrigin.X;
        long dy = (long)current.Y - _directionPreviewOrigin.Y;
        ulong absoluteX = (ulong)Math.Abs(dx);
        ulong absoluteY = (ulong)Math.Abs(dy);
        if (absoluteX == absoluteY)
        {
            DirectionPreviewProgress.Value = Math.Min(100, absoluteX * 100.0 / distance);
            DirectionPreviewText.Text = $"净位移 dx={dx}, dy={dy} px；主轴相等，方向尚未确定。";
            return;
        }

        MouseDirection observed;
        ulong primary;
        ulong offAxis;
        if (absoluteX > absoluteY)
        {
            observed = dx < 0 ? MouseDirection.Left : MouseDirection.Right;
            primary = absoluteX;
            offAxis = absoluteY;
        }
        else
        {
            observed = dy < 0 ? MouseDirection.Up : MouseDirection.Down;
            primary = absoluteY;
            offAxis = absoluteX;
        }

        DirectionPreviewProgress.Value = Math.Min(100, primary * 100.0 / distance);
        bool matches = observed == expected && primary >= distance && offAxis <= tolerance;
        DirectionPreviewText.Text = $"方向 {DirectionLabel(observed)}；净位移 dx={dx}, dy={dy} px；主轴 {primary} px，偏轴 {offAxis} px。";
        if (matches)
        {
            StopDirectionPreview($"预览命中 {DirectionLabel(observed)}；这里只显示摘要，不执行动作。", resetProgress: false);
            DirectionPreviewProgress.Value = 100;
        }
    }

    private void StopDirectionPreview(string message, bool resetProgress)
    {
        _directionPreviewTimer?.Stop();
        _directionPreviewTimer = null;
        _directionPreviewClock?.Stop();
        _directionPreviewClock = null;
        if (resetProgress) DirectionPreviewProgress.Value = 0;
        StartDirectionPreviewButton.IsEnabled = true;
        CancelDirectionPreviewButton.IsEnabled = false;
        if (message.Length > 0) DirectionPreviewText.Text = message;
    }

    private static string DirectionLabel(MouseDirection direction) => direction switch
    {
        MouseDirection.Left => "左",
        MouseDirection.Right => "右",
        MouseDirection.Up => "上",
        MouseDirection.Down => "下",
        _ => direction.ToString(),
    };

    private void AddActionKey_Click(object sender, RoutedEventArgs e) => AddActionPicker(KeyIdentity.Logical("A"));

    private void AddActionPicker(KeyIdentity key)
    {
        var picker = new KeyPicker();
        picker.SetIdentity(key);
        picker.CaptureRequested += KeyPicker_CaptureRequested;
        var remove = new Button { Content = "移除", VerticalAlignment = VerticalAlignment.Top };
        var row = new Grid { ColumnSpacing = 8 };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.Children.Add(picker);
        Grid.SetColumn(remove, 1);
        row.Children.Add(remove);
        remove.Click += (_, _) =>
        {
            picker.CaptureRequested -= KeyPicker_CaptureRequested;
            _actionPickers.Remove(picker);
            ActionKeysPanel.Children.Remove(row);
        };
        _actionPickers.Add(picker);
        ActionKeysPanel.Children.Add(row);
    }

    private void ClearActionPickers()
    {
        foreach (KeyPicker picker in _actionPickers) picker.CaptureRequested -= KeyPicker_CaptureRequested;
        _actionPickers.Clear();
        ActionKeysPanel.Children.Clear();
    }

    private async void KeyPicker_CaptureRequested(object? sender, EventArgs e)
    {
        if (sender is KeyPicker picker) await StartCaptureAsync(picker, mouseTarget: false);
    }

    private async void RecordMouseButton_Click(object sender, RoutedEventArgs e) =>
        await StartCaptureAsync(null, mouseTarget: true);

    private async Task StartCaptureAsync(KeyPicker? picker, bool mouseTarget)
    {
        StopDirectionPreview("方向预览已停止，正在录制输入。", resetProgress: false);
        if (_coordinator is null || _busy || !_captureIntent.TryBegin()) return;
        _captureKeyTarget = picker;
        _captureMouseTarget = mouseTarget;
        SetCaptureEditorLocked(true);
        CaptureBar.IsOpen = true;
        CaptureBar.Title = "正在启动录制";
        CaptureBar.Severity = InfoBarSeverity.Informational;
        CaptureBar.Message = "请按下一个输入。一次录制只填写当前字段；Esc 取消且不会写入草稿。";

        CaptureStarted started;
        try
        {
            started = await _coordinator.BeginCaptureAsync(10_000, _pageLifetime.Token);
        }
        catch (Exception error)
        {
            bool cancellationRequested = _captureIntent.FailBegin();
            FinishCaptureUi();
            ShowMessage(
                cancellationRequested ? "录制已取消" : "无法开始录制",
                cancellationRequested ? "取消意图已生效，没有输入会写入字段。" : error.Message,
                cancellationRequested ? InfoBarSeverity.Informational : InfoBarSeverity.Error);
            return;
        }

        CaptureBeginDisposition disposition = _captureIntent.CompleteBegin(started.SessionId);
        if (disposition == CaptureBeginDisposition.CancelImmediately)
        {
            await CancelKnownCaptureAsync(started.SessionId);
            return;
        }

        StartCaptureCountdown(started.SessionId, started.TimeoutMilliseconds);
        if (started.CompletedBeforeResponse is not null) HandleCaptureTerminal(started.CompletedBeforeResponse);
    }

    private void StartCaptureCountdown(ulong sessionId, int timeoutMilliseconds)
    {
        _captureCountdown?.Cancel();
        _captureCountdown = CancellationTokenSource.CreateLinkedTokenSource(_pageLifetime.Token);
        CancellationToken token = _captureCountdown.Token;
        _ = Task.Run(async () =>
        {
            int remaining = (int)Math.Ceiling(timeoutMilliseconds / 1000d);
            while (remaining > 0 && !token.IsCancellationRequested)
            {
                int shown = remaining--;
                DispatcherQueue.TryEnqueue(() =>
                {
                    if (_captureIntent.IsActiveSession(sessionId)) CaptureBar.Title = $"正在录制输入（剩余 {shown} 秒）";
                });
                await Task.Delay(1000, token).ConfigureAwait(false);
            }
        }, token);
    }

    private void HandleCaptureTerminal(CaptureTerminal terminal)
    {
        if (!_captureIntent.TryAcceptTerminal(terminal.SessionId)) return;
        if (terminal.Kind == CaptureTerminalKind.Captured && terminal.Input is CapturedKey key && key.Logical == "Escape")
        {
            FinishCaptureUi();
            ShowMessage("录制已取消", "Esc 保留为录制取消键；如需配置 Escape，请使用选择器。", InfoBarSeverity.Informational);
            return;
        }

        if (terminal.Kind == CaptureTerminalKind.Captured && _captureMouseTarget && terminal.Input is CapturedMouseButton mouse)
        {
            MouseButtonCombo.SelectedItem = mouse.Button;
            FinishCaptureUi();
            ShowMessage("录制完成", $"已选择鼠标按钮 {mouse.Button}。", InfoBarSeverity.Success);
            return;
        }

        if (terminal.Kind == CaptureTerminalKind.Captured && _captureKeyTarget is not null && terminal.Input is CapturedKey capturedKey)
        {
            _captureKeyTarget.SetCaptured(capturedKey);
            FinishCaptureUi();
            ShowMessage("录制完成", $"已记录 {capturedKey.Logical}；默认使用 logical identity。", InfoBarSeverity.Success);
            return;
        }

        string mismatch = terminal.Kind == CaptureTerminalKind.Captured
            ? "录制到的输入类型不适合当前字段，请重新录制。"
            : terminal.Message;
        FinishCaptureUi();
        ShowMessage("录制未完成", mismatch, InfoBarSeverity.Warning);
    }

    private async void CancelCapture_Click(object sender, RoutedEventArgs e) => await CancelCaptureBestEffortAsync();

    private async Task CancelCaptureBestEffortAsync()
    {
        if (_coordinator is null || !_captureIntent.IsInProgress) return;

        ulong? sessionId = _captureIntent.RequestCancel();
        InvalidateCaptureTarget();
        if (sessionId is null)
        {
            CaptureBar.IsOpen = true;
            CaptureBar.Title = "正在取消录制";
            CaptureBar.Message = "录制请求正在启动；取得 session ID 后会立即取消，期间不会写入任何字段。";
            return;
        }

        await CancelKnownCaptureAsync(sessionId.Value);
    }

    private async Task CancelKnownCaptureAsync(ulong sessionId)
    {
        using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(1));
        try
        {
            await _coordinator!.CancelCaptureAsync(sessionId, deadline.Token);
            ShowMessage("录制已取消", "取消键不会写入草稿。", InfoBarSeverity.Informational);
        }
        catch (Exception error)
        {
            // UI intent is already invalidated. Owner connection disposal and
            // the agent timeout are the final cancellation guarantees.
            ShowMessage(
                "取消意图已生效",
                $"当前字段已失效，不会写入录制结果；Agent 未确认取消：{error.Message}",
                InfoBarSeverity.Warning);
        }
        finally
        {
            _captureIntent.CompleteCancellation(sessionId);
            FinishCaptureUi();
        }
    }

    private async void Page_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Escape && _directionPreviewTimer is not null)
        {
            e.Handled = true;
            StopDirectionPreview("预览已由 Esc 取消；未保存坐标轨迹，也未执行动作。", resetProgress: false);
            return;
        }
        if (e.Key == VirtualKey.Escape && _captureIntent.IsInProgress)
        {
            e.Handled = true;
            await CancelCaptureBestEffortAsync();
        }
    }

    private async void CancelCaptureAccelerator_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (_directionPreviewTimer is not null)
        {
            args.Handled = true;
            StopDirectionPreview("预览已由 Esc 取消；未保存坐标轨迹，也未执行动作。", resetProgress: false);
            return;
        }
        if (!_captureIntent.IsInProgress) return;
        args.Handled = true;
        await CancelCaptureBestEffortAsync();
    }

    private void FinishCaptureUi()
    {
        _captureCountdown?.Cancel();
        _captureCountdown?.Dispose();
        _captureCountdown = null;
        InvalidateCaptureTarget();
        SetCaptureEditorLocked(false);
        CaptureBar.IsOpen = false;
        CaptureBar.Title = "正在录制输入";
    }

    private void InvalidateCaptureTarget()
    {
        _captureKeyTarget = null;
        _captureMouseTarget = false;
    }

    private void SetCaptureEditorLocked(bool locked)
    {
        RuleEditorScroll.IsEnabled = !locked;
        PrimaryKeyPicker.CaptureEnabled = !locked;
        SecondaryKeyPicker.CaptureEnabled = !locked;
        RecordMouseButtonButton.IsEnabled = !locked;
        foreach (KeyPicker picker in _actionPickers) picker.CaptureEnabled = !locked;
    }

    private void SetEmergencyKey_Click(object sender, RoutedEventArgs e)
    {
        if (!_draft.IsLoaded) return;
        if (!EmergencyKeyPicker.TryGetIdentity(out KeyIdentity? key, out string? error) || key?.Mode != KeyMatchMode.Logical)
        {
            ShowMessage("紧急键无效", error ?? "紧急旁路键必须使用 logical identity。", InfoBarSeverity.Error);
            return;
        }

        _draft.SetEmergencyKey(key);
        ShowMessage("已写入草稿", "紧急旁路键尚未保存；Agent 会在保存时检查规则冲突。", InfoBarSeverity.Informational);
    }

    private void GroupFilter_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!_refreshingRules) RefreshRuleList();
    }

    private void RefreshRuleList()
    {
        if (!_draft.IsLoaded) return;
        string selected = GroupFilter.SelectedItem as string ?? "全部规则";
        List<RuleRow> all = _draft.Draft.Rules
            .Select(rule => RuleRow.From(rule, (id, enabled) => _draft.SetRuleEnabled(id, enabled)))
            .ToList();
        List<string> groups = ["全部规则", .. all.Select(row => row.Group).Distinct().OrderBy(value => value, StringComparer.CurrentCulture)];
        _refreshingRules = true;
        GroupFilter.ItemsSource = groups;
        GroupFilter.SelectedItem = groups.Contains(selected) ? selected : "全部规则";
        selected = GroupFilter.SelectedItem as string ?? "全部规则";
        List<RuleRow> shown = selected == "全部规则" ? all : all.Where(row => row.Group == selected).ToList();
        RulesList.ItemsSource = shown;
        EmptyRulesText.Visibility = shown.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        _refreshingRules = false;
    }

    private void SetBusy(bool busy)
    {
        _busy = busy;
        SaveButton.IsEnabled = !busy && _coordinator?.Connection.ControlConnected == true;
        PauseButton.IsEnabled = !busy && _coordinator?.Connection.ControlConnected == true;
    }

    private void ShowMessage(string title, string message, InfoBarSeverity severity)
    {
        OperationBar.IsOpen = true;
        OperationBar.Title = title;
        OperationBar.Message = message;
        OperationBar.Severity = severity;
    }

    private void ShowEditorError(string message)
    {
        EditorInfo.Title = "规则字段有问题";
        EditorInfo.Message = message;
        EditorInfo.IsOpen = true;
    }

    private string SelectedTriggerType() =>
        (TriggerTypeCombo.SelectedItem as ComboBoxItem)?.Tag as string ?? "key_chord";

    private void SelectTriggerType(string type)
    {
        TriggerTypeCombo.SelectedItem = TriggerTypeCombo.Items
            .OfType<ComboBoxItem>()
            .First(item => string.Equals(item.Tag as string, type, StringComparison.Ordinal));
    }

    private void MainPage_SizeChanged(object sender, SizeChangedEventArgs e) => UpdateResponsiveEditor();

    private void UpdateResponsiveEditor()
    {
        bool narrowEditing = ActualWidth < 920 && RuleEditor.Visibility == Visibility.Visible;
        EditorColumn.Width = RuleEditor.Visibility == Visibility.Visible && !narrowEditing
            ? new GridLength(420)
            : new GridLength(0);
        Grid.SetColumn(RuleEditor, narrowEditing ? 0 : 1);
        Grid.SetColumnSpan(RuleEditor, narrowEditing ? 2 : 1);
        RulesListPane.Visibility = narrowEditing ? Visibility.Collapsed : Visibility.Visible;
    }

    private static string FormatPercentiles(PercentileSummary? summary) => summary is null || summary.Samples == 0
        ? "暂无样本"
        : $"n={summary.Samples}, p50={summary.P50}, p95={summary.P95}, p99={summary.P99}, max={summary.Max}";

    private static string SaveTitle(SaveResultKind kind) => kind switch
    {
        SaveResultKind.ValidationFailed => "验证失败",
        SaveResultKind.PersistenceFailed => "持久化失败",
        SaveResultKind.RuntimeCancelled => "运行时替换已取消",
        SaveResultKind.RuntimeFailed => "运行时替换失败",
        SaveResultKind.RuntimeBusy => "Agent 正在核对上一请求",
        SaveResultKind.RolledBackAfterTimeout => "保存已回滚",
        SaveResultKind.RecoveryRequired => "需要重启 Agent 恢复",
        _ => "保存结果待核对",
    };

    private static InfoBarSeverity SaveSeverity(SaveResultKind kind) => kind switch
    {
        SaveResultKind.ValidationFailed or SaveResultKind.RuntimeCancelled or SaveResultKind.RolledBackAfterTimeout => InfoBarSeverity.Warning,
        _ => InfoBarSeverity.Error,
    };

    private sealed class RuleRow
    {
        private readonly RuleEnablementBinding _enablement;

        private RuleRow(
            string id,
            bool enabled,
            string group,
            string triggerSummary,
            string actionSummary,
            Action<string, bool> enabledChanged)
        {
            Id = id;
            Group = group;
            TriggerSummary = triggerSummary;
            ActionSummary = actionSummary;
            _enablement = new RuleEnablementBinding(id, enabled, enabledChanged);
        }

        public string Id { get; }
        public bool Enabled { get => _enablement.Enabled; set => _enablement.Enabled = value; }
        public string Group { get; }
        public string TriggerSummary { get; }
        public string ActionSummary { get; }
        public string ToggleAutomationName => $"{Id} 启用状态";
        public string EditAutomationName => $"编辑 {Id}";
        public string DeleteAutomationName => $"删除 {Id}";
        public string RowAutomationName => $"规则 {Id}，{TriggerSummary}，{ActionSummary}";

        public override string ToString() => RowAutomationName;

        public static RuleRow From(RuleDocument rule, Action<string, bool> enabledChanged) => new(
            rule.Id,
            rule.Enabled,
            InputCatalog.GroupName(rule.Trigger.FirstKey),
            TriggerLabel(rule.Trigger),
            $"→ {string.Join(" + ", rule.Action.Keys.Select(KeyLabel))}",
            enabledChanged);

        private static string TriggerLabel(RuleTrigger trigger) => trigger switch
        {
            KeyChordTrigger chord => $"{KeyLabel(chord.First)} + {KeyLabel(chord.Second)}",
            KeyMouseButtonTrigger mouse => $"{KeyLabel(mouse.Key)} + 鼠标 {mouse.Button}",
            HoldTrigger hold => $"长按 {KeyLabel(hold.Key)} {hold.TimeoutMilliseconds} ms",
            HoldMouseButtonTrigger holdMouse => $"长按 {KeyLabel(holdMouse.Key)} {holdMouse.TimeoutMilliseconds} ms + 鼠标 {holdMouse.Button}",
            MouseDirectionTrigger direction =>
                $"按住 {KeyLabel(direction.Key)} + 鼠标{DirectionLabel(direction.Direction)} ≥{direction.MinimumDistancePixels}px / {direction.MaximumDurationMilliseconds}ms / 偏轴≤{direction.OffAxisTolerancePixels}px",
            _ => trigger.Type,
        };

        private static string KeyLabel(KeyIdentity key)
        {
            string display = KeyboardNameService.DisplayName(key);
            return display == key.StableName ? display : $"{display} [{key.StableName}]";
        }
    }
}
