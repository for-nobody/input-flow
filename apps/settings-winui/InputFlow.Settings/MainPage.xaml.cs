using System.Diagnostics;
using System.Reflection;
using System.Runtime.InteropServices;
using InputFlow.Protocol;
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
    private static readonly Version SettingsVersion =
        Assembly.GetExecutingAssembly().GetName().Version ?? new Version(0, 9, 0);
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
    private bool _initializingLanguage = true;

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
        Language = AppResources.CurrentLanguageTag;
        MouseButtonCombo.ItemsSource = InputCatalog.MouseButtons
            .Select(value => new LocalizedChoice<string>(value, MouseButtonLabel(value)))
            .ToList();
        SelectChoice(MouseButtonCombo, "Right");
        DirectionCombo.ItemsSource = InputCatalog.MouseDirections
            .Select(value => new LocalizedChoice<MouseDirection>(value, DirectionLabel(value)))
            .ToList();
        SelectChoice(DirectionCombo, MouseDirection.Right);
        TriggerTypeCombo.SelectedIndex = 0;
        LanguageCombo.SelectedItem = LanguageCombo.Items
            .OfType<ComboBoxItem>()
            .First(item => string.Equals(item.Tag as string, App.LanguagePreference, StringComparison.Ordinal));
        _initializingLanguage = false;
        UpdateVersionText();
        _draft.Changed += Draft_Changed;
        SizeChanged += MainPage_SizeChanged;
    }

    public bool HasUnsavedChanges => _draft.IsDirty || RuleEditor.Visibility == Visibility.Visible;

    internal string LocalizationSmokeRulesText => RulesNavigation.Content?.ToString() ?? string.Empty;

    internal string LocalizationSmokeNavigationName =>
        Microsoft.UI.Xaml.Automation.AutomationProperties.GetName(RootNavigation);

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
            Title = AppResources.Get("CloseDialog_Title"),
            Content = AppResources.Get("CloseDialog_Message"),
            PrimaryButtonText = AppResources.Get("CloseDialog_SaveAndExit"),
            SecondaryButtonText = AppResources.Get("CloseDialog_ExitWithoutSaving"),
            CloseButtonText = AppResources.Get("Common_Back"),
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
                ShowEditorError(error ?? AppResources.Get("RuleEditor_CannotCommit"));
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
            ShowMessage(AppResources.Get("Connection_NotConnectedTitle"), FriendlyError(error), InfoBarSeverity.Error);
        }
    }

    private void Page_Unloaded(object sender, RoutedEventArgs e)
    {
        StopDirectionPreview(AppResources.Get("Preview_Stopped"), resetProgress: false);
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
            StopDirectionPreview(AppResources.Get("Preview_ConnectionLost"), resetProgress: false);
        }
        ConnectionBar.IsOpen = true;
        ConnectionBar.Title = snapshot.Kind == AgentConnectionKind.Online
            ? AppResources.Get("Connection_OnlineTitle")
            : AppResources.Get("Connection_StatusTitle");
        ConnectionBar.Message = ConnectionMessage(snapshot.Kind);
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
        UpdateVersionText();
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
                    AppResources.Get("Authority_ChangedTitle"),
                    AppResources.Get("Authority_ChangedMessage"),
                    InfoBarSeverity.Warning);
            }
        }

        if (_coordinator?.LatestStatus is AgentStatus status)
        {
            string suspension = status.Suspended
                ? AppResources.Get("Status_Suspended")
                : AppResources.Get("Status_Running");
            string statusSummary = AppResources.Format(
                "Status_SummaryFormat",
                status.Phase,
                suspension,
                status.RuleCount,
                status.ApplyReconciliation);
            AgentStatusText.Text = statusSummary;
            AgentErrorText.Text = status.LastError ?? string.Empty;
            PauseButton.Label = status.Suspended
                ? AppResources.Get("Common_Resume")
                : AppResources.Get("Common_Pause");
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
                PauseButton,
                status.Suspended
                    ? AppResources.Get("PauseButton_ResumeAutomationName")
                    : AppResources.Get("PauseButton_PauseAutomationName"));
            PauseButton.Icon = new SymbolIcon(status.Suspended ? Symbol.Play : Symbol.Pause);
            if (_coordinator.Connection.Kind == AgentConnectionKind.Online)
            {
                ConnectionBar.IsOpen = true;
                ConnectionBar.Title = AppResources.Get("Connection_OnlineTitle");
                ConnectionBar.Message = status.LastError is null
                    ? statusSummary
                    : AppResources.Format("Status_WithLastErrorFormat", statusSummary, status.LastError);
                ConnectionBar.Severity = status.ApplyReconciliation == "recovery_required" || status.LastError is not null
                    ? InfoBarSeverity.Warning
                    : InfoBarSeverity.Success;
            }
        }
    }

    private void UpdateVersionText()
    {
        string agentVersion = _coordinator?.Handshake?.ServerVersion ?? AppResources.Get("Common_NotConnected");
        VersionText.Text = AppResources.Format(
            "About_VersionFormat",
            SettingsVersion.ToString(3),
            agentVersion,
            RuntimeInformation.ProcessArchitecture,
            ConfigDocument.CurrentSchemaVersion);
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
            ShowMessage(AppResources.Get("Reconnect_FailedTitle"), FriendlyError(error), InfoBarSeverity.Error);
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
                Title = AppResources.Get("ReloadDialog_Title"),
                Content = AppResources.Get("ReloadDialog_Message"),
                PrimaryButtonText = AppResources.Get("ReloadDialog_DiscardAndReload"),
                CloseButtonText = AppResources.Get("Common_Cancel"),
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
            ShowMessage(
                AppResources.Get("Reload_CompleteTitle"),
                AppResources.Get("Reload_CompleteMessage"),
                InfoBarSeverity.Success);
        }
        catch (Exception error)
        {
            ShowMessage(AppResources.Get("Reload_FailedTitle"), FriendlyError(error), InfoBarSeverity.Error);
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
            ShowEditorError(AppResources.Get("Save_FinishEditingFirst"));
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
                    Title = AppResources.Get("Save_ExternalChangeTitle"),
                    Content = AppResources.Get("Save_ExternalChangeMessage"),
                    PrimaryButtonText = AppResources.Get("Save_ConfirmOverwrite"),
                    SecondaryButtonText = AppResources.Get("Common_Reload"),
                    CloseButtonText = AppResources.Get("Save_KeepDraft"),
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

            string details = SaveMessage(result.Kind, result.CleanupWarnings.Count > 0);
            if (result.ValidationErrors.Count > 0)
            {
                details += $"\n• {string.Join("\n• ", result.ValidationErrors)}";
            }
            if (result.CleanupWarnings.Count > 0)
            {
                details += AppResources.Format(
                    "Save_CleanupWarningsFormat",
                    string.Join(AppResources.Get("Common_ListSeparator"), result.CleanupWarnings));
            }

            ShowMessage(
                result.IsApplied ? AppResources.Get("Save_CompleteTitle") : SaveTitle(result.Kind),
                details,
                result.IsApplied ? InfoBarSeverity.Success : SaveSeverity(result.Kind));
            return result;
        }
        catch (Exception error)
        {
            ShowMessage(
                AppResources.Get("Save_IncompleteTitle"),
                AppResources.Format("Save_IncompleteMessageFormat", FriendlyError(error)),
                InfoBarSeverity.Error);
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
                confirmed.Suspended
                    ? AppResources.Get("Runtime_PausedTitle")
                    : AppResources.Get("Runtime_ResumedTitle"),
                AppResources.Get("Runtime_ControlConfirmedMessage"),
                InfoBarSeverity.Success);
        }
        catch (Exception error)
        {
            ShowMessage(AppResources.Get("Runtime_ControlFailedTitle"), FriendlyError(error), InfoBarSeverity.Error);
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
            StatsText.Text = AppResources.Format(
                "Diagnostics_StatsFormat",
                stats.ObservedEvents,
                stats.OutputBatchesSent,
                stats.OutputBatchesFailed,
                stats.OutputBatchesDropped,
                FormatPercentiles(stats.CallbackLatencyMicroseconds),
                FormatPercentiles(stats.HoldDelayMicroseconds));
        }
        catch (Exception error)
        {
            StatsText.Text = AppResources.Format("Diagnostics_QueryFailedFormat", FriendlyError(error));
        }
        finally
        {
            SetBusy(false);
        }
    }

    private void LanguageCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializingLanguage || LanguageCombo.SelectedItem is not ComboBoxItem item ||
            item.Tag is not string language || !UiLanguagePreference.IsSupported(language))
        {
            return;
        }

        try
        {
            App.LanguagePreferenceStore.Save(language);
            LanguageRestartBar.Title = AppResources.Get("Language_RestartTitle");
            LanguageRestartBar.Message = AppResources.Get("Language_RestartMessage");
            LanguageRestartBar.IsOpen = true;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            LanguageRestartBar.IsOpen = false;
            ShowMessage(
                AppResources.Get("Language_SaveFailedTitle"),
                AppResources.Get("Language_SaveFailedMessage"),
                InfoBarSeverity.Error);
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
        EditorTitle.Text = AppResources.Get("RuleEditor_NewTitle");
        RuleIdBox.Text = $"rule-{Guid.NewGuid():N}"[..13];
        EditorEnabled.IsOn = true;
        TriggerTypeCombo.SelectedIndex = 0;
        PrimaryKeyPicker.SetIdentity(KeyIdentity.Logical("A"));
        SecondaryKeyPicker.SetIdentity(KeyIdentity.Logical("B"));
        SelectChoice(MouseButtonCombo, "Right");
        TimeoutBox.Value = 250;
        SelectChoice(DirectionCombo, MouseDirection.Right);
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
        EditorTitle.Text = AppResources.Get("RuleEditor_EditTitle");
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
                SelectChoice(MouseButtonCombo, mouse.Button);
                break;
            case HoldTrigger hold:
                TimeoutBox.Value = hold.TimeoutMilliseconds;
                break;
            case HoldMouseButtonTrigger holdMouse:
                TimeoutBox.Value = holdMouse.TimeoutMilliseconds;
                SelectChoice(MouseButtonCombo, holdMouse.Button);
                break;
            case MouseDirectionTrigger direction:
                SelectChoice(DirectionCombo, direction.Direction);
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
            Title = AppResources.Format("DeleteDialog_TitleFormat", id),
            Content = AppResources.Get("DeleteDialog_Message"),
            PrimaryButtonText = AppResources.Get("Common_Delete"),
            CloseButtonText = AppResources.Get("Common_Cancel"),
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
        if (type != "mouse_direction") StopDirectionPreview(AppResources.Get("Preview_DefaultMessage"), resetProgress: true);
        TimeoutHelpText.Text = type == "hold_mouse_button"
            ? AppResources.Get("RuleEditor_HoldMouseHelp")
            : AppResources.Get("RuleEditor_HoldHelp");
    }

    private void CommitEdit_Click(object sender, RoutedEventArgs e)
    {
        if (!TryBuildEditedRule(out RuleDocument? rule, out string? error))
        {
            ShowEditorError(error ?? AppResources.Get("RuleEditor_Incomplete"));
            return;
        }

        StopDirectionPreview(AppResources.Get("Preview_Stopped"), resetProgress: false);
        _draft.UpsertRule(rule!, _editingOriginalId);
        RuleEditor.Visibility = Visibility.Collapsed;
        UpdateResponsiveEditor();
    }

    private void CancelEdit_Click(object sender, RoutedEventArgs e)
    {
        StopDirectionPreview(AppResources.Get("Preview_CancelledShort"), resetProgress: false);
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
            error = AppResources.Get("RuleValidation_IdRequired");
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
            trigger = new KeyMouseButtonTrigger(first!, SelectedChoice(MouseButtonCombo, "Right"));
        }
        else if (type is "hold" or "hold_mouse_button")
        {
            if (double.IsNaN(TimeoutBox.Value) || TimeoutBox.Value is < 1 or > 60000)
            {
                error = AppResources.Get("RuleValidation_TimeoutRange");
                return false;
            }

            ulong timeout = (ulong)TimeoutBox.Value;
            trigger = type == "hold"
                ? new HoldTrigger(first!, timeout)
                : new HoldMouseButtonTrigger(first!, timeout, SelectedChoice(MouseButtonCombo, "Right"));
        }
        else
        {
            if (DirectionCombo.SelectedItem is not LocalizedChoice<MouseDirection> directionChoice)
            {
                error = AppResources.Get("RuleValidation_DirectionRequired");
                return false;
            }
            if (!TryDirectionParameters(out uint distance, out ulong duration, out uint tolerance, out error))
            {
                return false;
            }
            trigger = new MouseDirectionTrigger(first!, directionChoice.Value, distance, duration, tolerance);
        }

        var actionKeys = new List<KeyIdentity>();
        foreach (KeyPicker picker in _actionPickers)
        {
            if (!picker.TryGetIdentity(out KeyIdentity? key, out error)) return false;
            actionKeys.Add(key!);
        }

        if (actionKeys.Count == 0)
        {
            error = AppResources.Get("RuleValidation_ActionKeyRequired");
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
            error = AppResources.Get("RuleValidation_DirectionDistanceRange");
            return false;
        }
        if (double.IsNaN(durationValue) || durationValue is < 100 or > 5000 || durationValue != Math.Truncate(durationValue))
        {
            error = AppResources.Get("RuleValidation_DirectionDurationRange");
            return false;
        }
        if (double.IsNaN(toleranceValue) || toleranceValue is < 0 or > 2000 || toleranceValue != Math.Truncate(toleranceValue))
        {
            error = AppResources.Get("RuleValidation_DirectionToleranceRange");
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
            ShowEditorError(AppResources.Get("Preview_CaptureActive"));
            return;
        }
        if (DirectionCombo.SelectedItem is not LocalizedChoice<MouseDirection> expectedChoice)
        {
            ShowEditorError(AppResources.Get("RuleValidation_DirectionRequired"));
            return;
        }
        if (!TryDirectionParameters(out uint distance, out ulong duration, out uint tolerance, out string? error))
        {
            ShowEditorError(error ?? AppResources.Get("Preview_ParametersIncomplete"));
            return;
        }
        if (!GetCursorPos(out _directionPreviewOrigin))
        {
            ShowEditorError(AppResources.Get("Preview_CursorUnavailable"));
            return;
        }

        StopDirectionPreview("", resetProgress: true);
        EditorInfo.IsOpen = false;
        _directionPreviewClock = Stopwatch.StartNew();
        _directionPreviewTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(33) };
        _directionPreviewTimer.Tick += (_, _) => UpdateDirectionPreview(expectedChoice.Value, distance, duration, tolerance);
        _directionPreviewTimer.Start();
        StartDirectionPreviewButton.IsEnabled = false;
        CancelDirectionPreviewButton.IsEnabled = true;
        DirectionPreviewText.Text = AppResources.Format(
            "Preview_StartedFormat",
            _directionPreviewOrigin.X,
            _directionPreviewOrigin.Y,
            duration);
    }

    private void CancelDirectionPreview_Click(object sender, RoutedEventArgs e) =>
        StopDirectionPreview(AppResources.Get("Preview_Cancelled"), resetProgress: false);

    private void UpdateDirectionPreview(MouseDirection expected, uint distance, ulong duration, uint tolerance)
    {
        if (_directionPreviewClock is null || _directionPreviewClock.ElapsedMilliseconds > (long)duration)
        {
            StopDirectionPreview(AppResources.Get("Preview_TimedOut"), resetProgress: false);
            return;
        }
        if (!GetCursorPos(out CursorPoint current))
        {
            StopDirectionPreview(AppResources.Get("Preview_CursorReadFailed"), resetProgress: false);
            return;
        }

        long dx = (long)current.X - _directionPreviewOrigin.X;
        long dy = (long)current.Y - _directionPreviewOrigin.Y;
        ulong absoluteX = (ulong)Math.Abs(dx);
        ulong absoluteY = (ulong)Math.Abs(dy);
        if (absoluteX == absoluteY)
        {
            DirectionPreviewProgress.Value = Math.Min(100, absoluteX * 100.0 / distance);
            DirectionPreviewText.Text = AppResources.Format("Preview_AxisEqualFormat", dx, dy);
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
        DirectionPreviewText.Text = AppResources.Format(
            "Preview_ProgressFormat",
            DirectionLabel(observed),
            dx,
            dy,
            primary,
            offAxis);
        if (matches)
        {
            StopDirectionPreview(
                AppResources.Format("Preview_MatchedFormat", DirectionLabel(observed)),
                resetProgress: false);
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
        MouseDirection.Left => AppResources.Get("Direction_Left"),
        MouseDirection.Right => AppResources.Get("Direction_Right"),
        MouseDirection.Up => AppResources.Get("Direction_Up"),
        MouseDirection.Down => AppResources.Get("Direction_Down"),
        _ => direction.ToString(),
    };

    private void AddActionKey_Click(object sender, RoutedEventArgs e) => AddActionPicker(KeyIdentity.Logical("A"));

    private void AddActionPicker(KeyIdentity key)
    {
        var picker = new KeyPicker();
        picker.SetIdentity(key);
        picker.CaptureRequested += KeyPicker_CaptureRequested;
        var remove = new Button
        {
            Content = AppResources.Get("Common_Remove"),
            VerticalAlignment = VerticalAlignment.Top,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            remove,
            AppResources.Get("ActionKey_RemoveAutomationName"));
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
        StopDirectionPreview(AppResources.Get("Preview_StoppedForCapture"), resetProgress: false);
        if (_coordinator is null || _busy || !_captureIntent.TryBegin()) return;
        _captureKeyTarget = picker;
        _captureMouseTarget = mouseTarget;
        SetCaptureEditorLocked(true);
        CaptureBar.IsOpen = true;
        CaptureBar.Title = AppResources.Get("Capture_StartingTitle");
        CaptureBar.Severity = InfoBarSeverity.Informational;
        CaptureBar.Message = AppResources.Get("Capture_StartingMessage");

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
                cancellationRequested
                    ? AppResources.Get("Capture_CancelledTitle")
                    : AppResources.Get("Capture_StartFailedTitle"),
                cancellationRequested
                    ? AppResources.Get("Capture_CancelledBeforeStartMessage")
                    : FriendlyError(error),
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
                    if (_captureIntent.IsActiveSession(sessionId))
                    {
                        CaptureBar.Title = AppResources.Format("Capture_CountdownFormat", shown);
                    }
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
            ShowMessage(
                AppResources.Get("Capture_CancelledTitle"),
                AppResources.Get("Capture_EscapeReservedMessage"),
                InfoBarSeverity.Informational);
            return;
        }

        if (terminal.Kind == CaptureTerminalKind.Captured && _captureMouseTarget && terminal.Input is CapturedMouseButton mouse)
        {
            SelectChoice(MouseButtonCombo, mouse.Button);
            FinishCaptureUi();
            ShowMessage(
                AppResources.Get("Capture_CompleteTitle"),
                AppResources.Format("Capture_MouseCompleteFormat", MouseButtonLabel(mouse.Button)),
                InfoBarSeverity.Success);
            return;
        }

        if (terminal.Kind == CaptureTerminalKind.Captured && _captureKeyTarget is not null && terminal.Input is CapturedKey capturedKey)
        {
            _captureKeyTarget.SetCaptured(capturedKey);
            FinishCaptureUi();
            ShowMessage(
                AppResources.Get("Capture_CompleteTitle"),
                AppResources.Format("Capture_KeyCompleteFormat", capturedKey.Logical),
                InfoBarSeverity.Success);
            return;
        }

        string mismatch = terminal.Kind == CaptureTerminalKind.Captured
            ? AppResources.Get("Capture_TypeMismatchMessage")
            : CaptureTerminalMessage(terminal.Kind);
        FinishCaptureUi();
        ShowMessage(AppResources.Get("Capture_IncompleteTitle"), mismatch, InfoBarSeverity.Warning);
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
            CaptureBar.Title = AppResources.Get("Capture_CancellingTitle");
            CaptureBar.Message = AppResources.Get("Capture_CancellingPendingMessage");
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
            ShowMessage(
                AppResources.Get("Capture_CancelledTitle"),
                AppResources.Get("Capture_CancelledMessage"),
                InfoBarSeverity.Informational);
        }
        catch (Exception error)
        {
            // UI intent is already invalidated. Owner connection disposal and
            // the agent timeout are the final cancellation guarantees.
            ShowMessage(
                AppResources.Get("Capture_CancelIntentTitle"),
                AppResources.Format("Capture_CancelUnconfirmedFormat", FriendlyError(error)),
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
            StopDirectionPreview(AppResources.Get("Preview_EscapeCancelled"), resetProgress: false);
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
            StopDirectionPreview(AppResources.Get("Preview_EscapeCancelled"), resetProgress: false);
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
        CaptureBar.Title = AppResources.Get("CaptureBar_Title");
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
            ShowMessage(
                AppResources.Get("Emergency_InvalidTitle"),
                error ?? AppResources.Get("Emergency_LogicalRequiredMessage"),
                InfoBarSeverity.Error);
            return;
        }

        _draft.SetEmergencyKey(key);
        ShowMessage(
            AppResources.Get("Draft_UpdatedTitle"),
            AppResources.Get("Emergency_DraftUpdatedMessage"),
            InfoBarSeverity.Informational);
    }

    private void GroupFilter_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!_refreshingRules) RefreshRuleList();
    }

    private void RefreshRuleList()
    {
        if (!_draft.IsLoaded) return;
        string allRules = AppResources.Get("RuleGroup_All");
        string selected = GroupFilter.SelectedItem as string ?? allRules;
        List<RuleRow> all = _draft.Draft.Rules
            .Select(rule => RuleRow.From(rule, (id, enabled) => _draft.SetRuleEnabled(id, enabled)))
            .ToList();
        List<string> groups = [allRules, .. all.Select(row => row.Group).Distinct().OrderBy(value => value, StringComparer.CurrentCulture)];
        _refreshingRules = true;
        GroupFilter.ItemsSource = groups;
        GroupFilter.SelectedItem = groups.Contains(selected) ? selected : allRules;
        selected = GroupFilter.SelectedItem as string ?? allRules;
        List<RuleRow> shown = selected == allRules ? all : all.Where(row => row.Group == selected).ToList();
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
        EditorInfo.Title = AppResources.Get("RuleEditor_ErrorTitle");
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
        ? AppResources.Get("Diagnostics_NoSamples")
        : $"n={summary.Samples}, p50={summary.P50}, p95={summary.P95}, p99={summary.P99}, max={summary.Max}";

    private static string ConnectionMessage(AgentConnectionKind kind) => kind switch
    {
        AgentConnectionKind.Connecting => AppResources.Get("Connection_ConnectingMessage"),
        AgentConnectionKind.Online => AppResources.Get("Connection_OnlineMessage"),
        AgentConnectionKind.EventStreamReconnecting => AppResources.Get("Connection_ReconnectingMessage"),
        _ => AppResources.Get("Connection_OfflineMessage"),
    };

    private static string CaptureTerminalMessage(CaptureTerminalKind kind) => kind switch
    {
        CaptureTerminalKind.Cancelled => AppResources.Get("Capture_CancelledMessage"),
        CaptureTerminalKind.TimedOut => AppResources.Get("Capture_TimedOutMessage"),
        CaptureTerminalKind.AgentShutdown => AppResources.Get("Capture_AgentShutdownMessage"),
        CaptureTerminalKind.EventStreamLost => AppResources.Get("Capture_EventStreamLostMessage"),
        _ => AppResources.Get("Capture_TypeMismatchMessage"),
    };

    private static string FriendlyError(Exception error) => error switch
    {
        TimeoutException => AppResources.Get("Error_Timeout"),
        UnauthorizedAccessException => AppResources.Get("Error_AccessDenied"),
        ProtocolException protocol when protocol.Code == "unsupported_protocol" =>
            AppResources.Get("Error_UnsupportedProtocol"),
        ProtocolException protocol when protocol.Code == "unsupported_schema" =>
            AppResources.Get("Error_UnsupportedSchema"),
        ProtocolException protocol => AppResources.Format("Error_ProtocolFormat", protocol.Code),
        IOException => AppResources.Get("Error_ConnectionInterrupted"),
        InvalidOperationException => AppResources.Get("Error_InvalidState"),
        FormatException => AppResources.Get("Error_InvalidAgentData"),
        _ => AppResources.Format("Error_UnexpectedFormat", error.GetType().Name),
    };

    private static string MouseButtonLabel(string button) => button switch
    {
        "Left" => AppResources.Get("MouseButton_Left"),
        "Right" => AppResources.Get("MouseButton_Right"),
        "Middle" => AppResources.Get("MouseButton_Middle"),
        "XButton1" => AppResources.Get("MouseButton_XButton1"),
        "XButton2" => AppResources.Get("MouseButton_XButton2"),
        _ => button,
    };

    private static string KeyGroupLabel(KeyIdentity key)
    {
        if (key.Mode == KeyMatchMode.Physical)
        {
            return key.StableName;
        }

        string logical = key.LogicalKey!;
        if (logical is "LeftCtrl" or "RightCtrl") return "Ctrl";
        if (logical is "LeftShift" or "RightShift") return "Shift";
        if (logical is "LeftAlt" or "RightAlt") return "Alt";
        if (logical.StartsWith("Oem", StringComparison.Ordinal)) return AppResources.Get("RuleGroup_Symbols");
        if (logical.StartsWith("Digit", StringComparison.Ordinal)) return AppResources.Get("RuleGroup_Digits");
        if (logical.StartsWith("Numpad", StringComparison.Ordinal)) return AppResources.Get("RuleGroup_Numpad");
        if (logical.Length == 1 && char.IsAsciiLetter(logical[0])) return AppResources.Get("RuleGroup_Letters");
        return logical == "Space" ? AppResources.Get("RuleGroup_Space") : InputCatalog.GroupName(key);
    }

    private static void SelectChoice<T>(ComboBox comboBox, T value)
    {
        comboBox.SelectedItem = comboBox.Items
            .OfType<LocalizedChoice<T>>()
            .FirstOrDefault(choice => EqualityComparer<T>.Default.Equals(choice.Value, value));
    }

    private static T SelectedChoice<T>(ComboBox comboBox, T fallback) =>
        comboBox.SelectedItem is LocalizedChoice<T> choice ? choice.Value : fallback;

    private static string SaveTitle(SaveResultKind kind) => kind switch
    {
        SaveResultKind.ValidationFailed => AppResources.Get("SaveTitle_ValidationFailed"),
        SaveResultKind.PersistenceFailed => AppResources.Get("SaveTitle_PersistenceFailed"),
        SaveResultKind.RuntimeCancelled => AppResources.Get("SaveTitle_RuntimeCancelled"),
        SaveResultKind.RuntimeFailed => AppResources.Get("SaveTitle_RuntimeFailed"),
        SaveResultKind.RuntimeBusy => AppResources.Get("SaveTitle_RuntimeBusy"),
        SaveResultKind.RolledBackAfterTimeout => AppResources.Get("SaveTitle_RolledBack"),
        SaveResultKind.RecoveryRequired => AppResources.Get("SaveTitle_RecoveryRequired"),
        _ => AppResources.Get("SaveTitle_Unknown"),
    };

    private static string SaveMessage(SaveResultKind kind, bool hasCleanupWarnings) => kind switch
    {
        SaveResultKind.Applied when hasCleanupWarnings => AppResources.Get("SaveMessage_AppliedWithWarnings"),
        SaveResultKind.Applied => AppResources.Get("SaveMessage_Applied"),
        SaveResultKind.ExternalChangeDetected => AppResources.Get("Save_ExternalChangeMessage"),
        SaveResultKind.ValidationFailed => AppResources.Get("SaveMessage_ValidationFailed"),
        SaveResultKind.PersistenceFailed => AppResources.Get("SaveMessage_PersistenceFailed"),
        SaveResultKind.RuntimeCancelled => AppResources.Get("SaveMessage_RuntimeCancelled"),
        SaveResultKind.RuntimeFailed => AppResources.Get("SaveMessage_RuntimeFailed"),
        SaveResultKind.RuntimeBusy => AppResources.Get("SaveMessage_RuntimeBusy"),
        SaveResultKind.RolledBackAfterTimeout => AppResources.Get("SaveMessage_RolledBack"),
        SaveResultKind.RecoveryRequired => AppResources.Get("SaveMessage_RecoveryRequired"),
        _ => AppResources.Get("SaveMessage_Unknown"),
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
        public string ToggleAutomationName => AppResources.Format("RuleRow_ToggleAutomationNameFormat", Id);
        public string EditAutomationName => AppResources.Format("RuleRow_EditAutomationNameFormat", Id);
        public string DeleteAutomationName => AppResources.Format("RuleRow_DeleteAutomationNameFormat", Id);
        public string RowAutomationName => AppResources.Format(
            "RuleRow_AutomationNameFormat",
            Id,
            TriggerSummary,
            ActionSummary);

        public override string ToString() => RowAutomationName;

        public static RuleRow From(RuleDocument rule, Action<string, bool> enabledChanged) => new(
            rule.Id,
            rule.Enabled,
            KeyGroupLabel(rule.Trigger.FirstKey),
            TriggerLabel(rule.Trigger),
            AppResources.Format("RuleRow_ActionFormat", string.Join(" + ", rule.Action.Keys.Select(KeyLabel))),
            enabledChanged);

        private static string TriggerLabel(RuleTrigger trigger) => trigger switch
        {
            KeyChordTrigger chord => $"{KeyLabel(chord.First)} + {KeyLabel(chord.Second)}",
            KeyMouseButtonTrigger mouse => AppResources.Format(
                "RuleRow_KeyMouseTriggerFormat",
                KeyLabel(mouse.Key),
                MouseButtonLabel(mouse.Button)),
            HoldTrigger hold => AppResources.Format(
                "RuleRow_HoldTriggerFormat",
                KeyLabel(hold.Key),
                hold.TimeoutMilliseconds),
            HoldMouseButtonTrigger holdMouse => AppResources.Format(
                "RuleRow_HoldMouseTriggerFormat",
                KeyLabel(holdMouse.Key),
                holdMouse.TimeoutMilliseconds,
                MouseButtonLabel(holdMouse.Button)),
            MouseDirectionTrigger direction =>
                AppResources.Format(
                    "RuleRow_MouseDirectionTriggerFormat",
                    KeyLabel(direction.Key),
                    DirectionLabel(direction.Direction),
                    direction.MinimumDistancePixels,
                    direction.MaximumDurationMilliseconds,
                    direction.OffAxisTolerancePixels),
            _ => trigger.Type,
        };

        private static string KeyLabel(KeyIdentity key)
        {
            string display = KeyboardNameService.DisplayName(key);
            return display == key.StableName ? display : $"{display} [{key.StableName}]";
        }
    }

    private sealed record LocalizedChoice<T>(T Value, string Label)
    {
        public override string ToString() => Label;
    }
}
