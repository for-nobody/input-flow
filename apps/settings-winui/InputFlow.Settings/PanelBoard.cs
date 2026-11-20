using InputFlow.Settings.Core;
using Microsoft.UI;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Foundation;
using Windows.UI;

namespace InputFlow_Settings;

internal sealed class PanelBoard : UserControl
{
    private static readonly Brush Outline = BrushFrom(0xff8490a3);
    private static readonly Brush BoardFill = BrushFrom(0xff181e2a);
    private static readonly Brush RegionFill = BrushFrom(0xff303a4b);
    private static readonly Brush EmptyFill = BrushFrom(0xff232b39);
    private static readonly Brush SelectionFill = BrushFrom(0xff234b83);
    private static readonly Brush ActiveEdge = BrushFrom(0xff70e2da);
    private readonly Canvas _canvas = new() { HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
    private readonly Border _frame;
    private readonly bool _labels;
    private PanelGeometry? _geometry;
    private IReadOnlySet<string> _selected = new HashSet<string>();
    private PanelEdge? _edge;
    private PanelEdge? _line;
    private PanelEdge? _invalidRectangle;
    private bool _preview;
    private double _aspect = 1.67;

    public PanelBoard(bool labels)
    {
        _labels = labels;
        _frame = new Border
        {
            Background = BoardFill, BorderBrush = Outline, BorderThickness = new Thickness(1),
            Child = _canvas, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center,
        };
        var root = new Grid();
        root.Children.Add(_frame);
        Content = root;
        IsTabStop = labels;
        AutomationProperties.SetName(this, AppResources.Get(labels ? "Panel_Canvas" : "Panel_Preview"));
        SizeChanged += (_, _) => Redraw();
    }

    public PanelPoint FromPoint(Point point)
    {
        Point relative = TransformToVisual(_canvas).TransformPoint(point);
        return new(relative.X / _canvas.Width, relative.Y / _canvas.Height);
    }

    public void SetDeviceSize(double? width, double? height)
    {
        double aspect = width is > 0 && height is > 0 ? width.Value / height.Value : 1.67;
        if (!double.IsFinite(aspect) || aspect <= 0 || _aspect == aspect) return;
        _aspect = aspect;
        Redraw();
    }

    public static string DimensionsText(double? width, double? height) =>
        width is > 0 && height is > 0 ? AppResources.Format("Panel_Dimensions", width, height) : AppResources.Get("Panel_Schematic");

    public void SetEdgeCursor(PanelEdgeHandle? edge) => ProtectedCursor = edge switch
    {
        PanelRectangleEdge { IsVertical: true } => InputSystemCursor.Create(InputSystemCursorShape.SizeWestEast),
        PanelRectangleEdge => InputSystemCursor.Create(InputSystemCursorShape.SizeNorthSouth),
        PanelSeam => InputSystemCursor.Create(InputSystemCursorShape.SizeAll),
        _ => null,
    };

    public void Render(PanelGeometry geometry, IReadOnlySet<string>? selected = null, PanelEdge? edge = null,
        PanelEdge? line = null, bool preview = false, PanelEdge? invalidRectangle = null)
    {
        _geometry = geometry;
        _selected = selected ?? _selected;
        _edge = edge;
        _line = line;
        _invalidRectangle = invalidRectangle;
        _preview = preview;
        Redraw();
    }

    private void Redraw()
    {
        if (ActualWidth < 8 || ActualHeight < 8 || _geometry is null) return;
        _canvas.Width = Math.Min(ActualWidth - 4, (ActualHeight - 4) * _aspect);
        _canvas.Height = _canvas.Width / _aspect;
        _canvas.Children.Clear();
        Point Pixel(PanelPoint point) => new(point.X * _canvas.Width, point.Y * _canvas.Height);
        foreach (PanelRegion region in _geometry.Regions)
        {
            Brush fill = _selected.Contains(region.Id) ? SelectionFill : region.Action is null ? EmptyFill : RegionFill;
            foreach (IReadOnlyList<PanelPoint> face in region.Faces)
            {
                var polygon = new Polygon { Fill = fill, IsHitTestVisible = false };
                foreach (PanelPoint point in face) polygon.Points.Add(Pixel(point));
                _canvas.Children.Add(polygon);
            }
            foreach (PanelEdge boundary in PanelGeometryEditor.Outline(region))
                DrawLine(Pixel(boundary.Start), Pixel(boundary.End), Outline, 1);
            if (_labels) DrawLabel(region, Pixel);
        }
        foreach (PanelSeam seam in new PanelSurface(_geometry).Seams)
        {
            Point start = Pixel(seam.Edge.Start);
            Point end = Pixel(seam.Edge.End);
            double dx = end.X - start.X;
            double dy = end.Y - start.Y;
            double length = Math.Sqrt(dx * dx + dy * dy);
            double width = length == 0 ? 1 : 2 * Math.Sqrt(
                Math.Pow(dy * _geometry.Gap.X * _canvas.Width, 2) +
                Math.Pow(dx * _geometry.Gap.Y * _canvas.Height, 2)) / length;
            DrawLine(start, end, BoardFill, Math.Max(1, width));
            DrawLine(start, end, Outline, 1);
        }
        if (_edge is { } highlighted) DrawLine(Pixel(highlighted.Start), Pixel(highlighted.End), ActiveEdge, 3);
        if (_line is { } trace) DrawLine(Pixel(trace.Start), Pixel(trace.End), ActiveEdge, 2, true);
        if (_invalidRectangle is { } corners)
        {
            var box = PanelGeometryEditor.Rectangle(corners.Start.X, corners.Start.Y, corners.End.X, corners.End.Y);
            foreach (PanelEdge edge in PanelGeometryEditor.Edges(box))
                DrawLine(Pixel(edge.Start), Pixel(edge.End), BrushFrom(0xffff8888), 2, true);
        }
        _frame.BorderBrush = _preview ? ActiveEdge : Outline;
    }

    private void DrawLine(Point start, Point end, Brush brush, double width, bool dashed = false)
    {
        var line = new Line
        {
            X1 = start.X, Y1 = start.Y, X2 = end.X, Y2 = end.Y,
            Stroke = brush, StrokeThickness = width, IsHitTestVisible = false,
        };
        if (dashed) line.StrokeDashArray = new DoubleCollection { 4, 3 };
        _canvas.Children.Add(line);
    }

    private void DrawLabel(PanelRegion region, Func<PanelPoint, Point> pixel)
    {
        IReadOnlyList<PanelPoint> face = region.Faces.MaxBy(face =>
            Math.Abs(PanelGeometryEditor.Edges(face).Sum(edge => edge.Start.X * edge.End.Y - edge.Start.Y * edge.End.X)))!;
        var center = new PanelPoint(face.Average(point => point.X), face.Average(point => point.Y));
        double width = face.Max(point => point.X) - face.Min(point => point.X);
        double height = face.Max(point => point.Y) - face.Min(point => point.Y);
        PanelEdge[] edges = PanelGeometryEditor.Edges(face).ToArray();
        var box = PanelGeometryEditor.Rectangle(center.X - width / 2, center.Y - height / 2, center.X + width / 2, center.Y + height / 2);
        double scale = 1;
        foreach (PanelPoint corner in box)
        foreach (PanelEdge edge in edges)
        {
            double inside = edge.Side(center);
            double outside = edge.Side(corner);
            if (outside < 0) scale = Math.Min(scale, inside / (inside - outside));
        }
        Point topLeft = pixel(new(center.X - width * scale / 2, center.Y - height * scale / 2));
        double contentWidth = width * scale * _canvas.Width;
        double contentHeight = height * scale * _canvas.Height;
        if (contentWidth < 18 || contentHeight < 19) return;
        string text = contentHeight >= 42
            ? AppResources.Format("Panel_RegionLabel", region.Name, ActionLabel(region.Action))
            : region.Name;
        var label = new TextBlock
        {
            Text = text, Foreground = new SolidColorBrush(Colors.White), FontSize = 13,
            TextAlignment = TextAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis,
            TextWrapping = TextWrapping.Wrap, Width = Math.Max(1, contentWidth - 6),
            MaxLines = contentHeight >= 42 ? 2 : 1, IsHitTestVisible = false,
        };
        Canvas.SetLeft(label, topLeft.X + 3);
        Canvas.SetTop(label, topLeft.Y + Math.Max(1, (contentHeight - 34) / 2));
        _canvas.Children.Add(label);
    }

    public static string ActionLabel(PanelAction? action) => action switch
    {
        PanelDigit digit => AppResources.Format("Panel_ActionDigitLabel", digit.Digit),
        PanelArithmetic arithmetic => AppResources.Format("Panel_ActionOperatorLabel", OperatorSymbol(arithmetic.Operator)),
        PanelKeys keys => AppResources.Format("Panel_ActionKeysLabel", string.Join(" + ", keys.Keys.Select(KeyboardNameService.DisplayName))),
        _ => AppResources.Get("Presentation_Unbound"),
    };

    public static string OperatorSymbol(PanelOperator op) => op switch
    {
        PanelOperator.Add => "+",
        PanelOperator.Subtract => "−",
        PanelOperator.Multiply => "×",
        PanelOperator.Divide => "÷",
        _ => string.Empty,
    };

    private static Brush BrushFrom(uint argb) =>
        new SolidColorBrush(Color.FromArgb((byte)(argb >> 24), (byte)(argb >> 16), (byte)(argb >> 8), (byte)argb));
}
