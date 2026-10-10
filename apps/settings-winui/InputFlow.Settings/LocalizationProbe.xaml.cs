using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace InputFlow_Settings;

internal sealed partial class LocalizationProbe : UserControl
{
    public LocalizationProbe()
    {
        InitializeComponent();
    }

    public string StaticText => ProbeText.Text;

    public string AutomationName => AutomationProperties.GetName(ProbeButton);
}
