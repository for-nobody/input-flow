namespace InputFlow.Settings.Core;

public readonly record struct PanelEdge(PanelPoint Start, PanelPoint End)
{
    public PanelPoint At(double fraction) => Start + (End - Start) * fraction;
    public double Side(PanelPoint point) => (End.X - Start.X) * (point.Y - Start.Y) - (End.Y - Start.Y) * (point.X - Start.X);
    public double Length => Math.Sqrt(Dot(End - Start, End - Start));

    private static double Dot(PanelPoint a, PanelPoint b) => a.X * b.X + a.Y * b.Y;

    public double Projection(PanelPoint point) => Dot(point - Start, End - Start) / Dot(End - Start, End - Start);
}

public abstract record PanelEdgeHandle(PanelEdge Edge);
public sealed record PanelSeam(PanelEdge Edge, string First, string Second) : PanelEdgeHandle(Edge);
public sealed record PanelRectangleEdge(PanelEdge Edge, string RegionId) : PanelEdgeHandle(Edge)
{
    public bool IsVertical => Edge.Start.X == Edge.End.X;
}
public readonly record struct PanelHit(string? RegionId, bool Gap);
public sealed record PanelEdit(PanelGeometry? Geometry, string? Error)
{
    public bool Changed => Geometry is not null;
}

public sealed class PanelSurface
{
    private const double EdgeHitTolerance = .012;
    private readonly PanelGeometry _geometry;
    private PanelRectangleEdge[]? _rectangleEdges;
    public IReadOnlyList<PanelSeam> Seams { get; }

    public PanelSurface(PanelGeometry geometry)
    {
        _geometry = geometry;
        List<PanelSeam> seams = [];
        for (int i = 0; i < geometry.Regions.Count; i++)
        {
            PanelRegion region = geometry.Regions[i];
            foreach (PanelRegion other in geometry.Regions.Take(i))
            {
                foreach (PanelEdge edge in region.Faces.SelectMany(PanelGeometryEditor.Edges))
                foreach (PanelEdge neighbor in other.Faces.SelectMany(PanelGeometryEditor.Edges))
                {
                    if (PanelGeometryEditor.SharedInterval(edge, neighbor) is { } interval)
                        seams.Add(new(new(edge.At(interval.Start), edge.At(interval.End)), region.Id, other.Id));
                }
            }
        }
        Seams = seams;
    }

    public PanelEdgeHandle? FindEdge(PanelPoint point)
    {
        if (!double.IsFinite(point.X) || !double.IsFinite(point.Y)) return null;
        return Nearest(Seams) ?? Nearest(_rectangleEdges ??= RectangleEdges().ToArray());

        PanelEdgeHandle? Nearest(IEnumerable<PanelEdgeHandle> edges) => edges
            .Where(handle => handle.Edge.Projection(point) is >= -EdgeHitTolerance and <= 1 + EdgeHitTolerance)
            .Select(handle => (Handle: handle, Distance: Math.Abs(handle.Edge.Side(point)) / handle.Edge.Length))
            .Where(match => match.Distance < EdgeHitTolerance)
            .OrderBy(match => match.Distance).Select(match => match.Handle).FirstOrDefault();
    }

    private IEnumerable<PanelRectangleEdge> RectangleEdges()
    {
        foreach (PanelRegion region in _geometry.Regions)
        {
            if (PanelGeometryEditor.RectangleOutline(region) is not { } rectangle) continue;
            foreach (PanelEdge edge in PanelGeometryEditor.Edges(rectangle))
            {
                if (Seams.Any(seam => (seam.First == region.Id || seam.Second == region.Id)
                    && PanelGeometryEditor.SharedInterval(edge, seam.Edge) is not null)) continue;
                yield return new PanelRectangleEdge(edge, region.Id);
            }
        }
    }

    public PanelHit Hit(PanelPoint point)
    {
        if (!PanelGeometryEditor.OnBoard(point)) return default;
        foreach (PanelSeam seam in Seams)
        {
            PanelPoint Scale(PanelPoint p) => new(
                p.X / Math.Max(_geometry.Gap.X, PanelGeometryEditor.Epsilon),
                p.Y / Math.Max(_geometry.Gap.Y, PanelGeometryEditor.Epsilon));
            var edge = new PanelEdge(Scale(seam.Edge.Start), Scale(seam.Edge.End));
            PanelPoint scaled = Scale(point);
            if (edge.Projection(scaled) is >= -PanelGeometryEditor.Epsilon and <= 1 + PanelGeometryEditor.Epsilon
                && Math.Abs(edge.Side(scaled)) <= edge.Length * (1 + PanelGeometryEditor.Epsilon))
                return new(null, true);
        }
        return new(_geometry.Regions.FirstOrDefault(region => region.Faces.Any(face =>
            PanelGeometryEditor.Edges(face).All(edge => edge.Side(point) >= -PanelGeometryEditor.Epsilon)))?.Id, false);
    }
}

public static class PanelGeometryEditor
{
    internal const double Epsilon = 1e-8;

    public static IReadOnlyList<PanelPoint> Rectangle(double left, double top, double right, double bottom) =>
        [new(left, top), new(right, top), new(right, bottom), new(left, bottom)];

    public static IEnumerable<PanelEdge> Edges(IReadOnlyList<PanelPoint> face)
    {
        for (int i = 0; i < face.Count; i++) yield return new(face[i], face[(i + 1) % face.Count]);
    }

    internal static bool OnBoard(PanelPoint point) => point.X is >= 0 and <= 1 && point.Y is >= 0 and <= 1;

    internal static (double Start, double End)? SharedInterval(PanelEdge a, PanelEdge b)
    {
        if (Math.Abs(a.Side(b.Start)) > Epsilon * a.Length || Math.Abs(a.Side(b.End)) > Epsilon * a.Length)
            return null;
        double c = a.Projection(b.Start);
        double d = a.Projection(b.End);
        double start = Math.Max(0, Math.Min(c, d));
        double end = Math.Min(1, Math.Max(c, d));
        return end - start > Epsilon ? (start, end) : null;
    }

    // Remove only shared portions inside a region, including unmatched subdivisions.
    public static IEnumerable<PanelEdge> Outline(PanelRegion region)
    {
        for (int i = 0; i < region.Faces.Count; i++)
        {
            foreach (PanelEdge edge in Edges(region.Faces[i]))
            {
                var hidden = region.Faces.Where((_, index) => index != i).SelectMany(Edges)
                    .Select(neighbor => SharedInterval(edge, neighbor)).OfType<(double Start, double End)>()
                    .OrderBy(interval => interval.Start);
                double cursor = 0;
                foreach (var interval in hidden)
                {
                    if (interval.Start - cursor > Epsilon) yield return new(edge.At(cursor), edge.At(interval.Start));
                    cursor = Math.Max(cursor, interval.End);
                }
                if (1 - cursor > Epsilon) yield return new(edge.At(cursor), edge.End);
            }
        }
    }

    internal static IReadOnlyList<PanelPoint>? RectangleOutline(PanelRegion region)
    {
        PanelEdge[] outline = Outline(region).ToArray();
        if (outline.Length < 4) return null;
        double left = outline.Min(edge => edge.Start.X);
        double right = outline.Max(edge => edge.Start.X);
        double top = outline.Min(edge => edge.Start.Y);
        double bottom = outline.Max(edge => edge.Start.Y);
        bool rectangular = outline.All(edge =>
            (edge.Start.X == edge.End.X && (edge.Start.X == left || edge.Start.X == right))
            || (edge.Start.Y == edge.End.Y && (edge.Start.Y == top || edge.Start.Y == bottom)));
        return rectangular ? Rectangle(left, top, right, bottom) : null;
    }

    public static string? Validate(PanelGeometry geometry)
    {
        if (geometry.Gap.X is not (>= 0 and <= .04) || geometry.Gap.Y is not (>= 0 and <= .04)
            || geometry.Regions.Count > 64 || geometry.Regions.Sum(region => region.Faces.Count) > 128)
            return "Panel_InvalidGeometry";

        List<IReadOnlyList<PanelPoint>> previous = [];
        HashSet<string> ids = new(StringComparer.Ordinal);
        foreach (PanelRegion region in geometry.Regions)
        {
            if (!ValidText(region.Id, 64) || !ids.Add(region.Id) || !ValidText(region.Name, 80) || region.Faces.Count == 0)
                return "Panel_InvalidRegion";
            foreach (IReadOnlyList<PanelPoint> face in region.Faces)
            {
                if (!ValidFace(face)) return "Panel_InvalidPolygon";
                if (previous.Any(other => Overlaps(face, other))) return "Panel_Overlap";
                previous.Add(face);
            }
            if (region.Action is PanelDigit { Digit: < 0 or > 9 } ||
                region.Action is PanelKeys keys &&
                (keys.Keys.Count is < 1 or > 8 || keys.Keys.Distinct().Count() != keys.Keys.Count
                || keys.Keys.Any(key => key.LogicalKey == "NumLock" || key.ScanCode == 0x45)))
                return "Panel_InvalidAction";
        }
        return null;
    }

    public static bool ValidText(string text, int limit) =>
        !string.IsNullOrWhiteSpace(text) && text.Length <= limit && !text.Any(char.IsControl);

    public static bool ContentEquals(PanelGeometry a, PanelGeometry b) =>
        ReferenceEquals(a, b) || a.Gap == b.Gap && a.Regions.Count == b.Regions.Count
        && a.Regions.Zip(b.Regions).All(pair =>
            pair.First.Id == pair.Second.Id && pair.First.Name == pair.Second.Name
            && ActionsEqual(pair.First.Action, pair.Second.Action)
            && pair.First.Faces.Count == pair.Second.Faces.Count
            && pair.First.Faces.Zip(pair.Second.Faces).All(faces => faces.First.SequenceEqual(faces.Second)));

    public static bool ActionsEqual(PanelAction? a, PanelAction? b) =>
        a is PanelKeys aKeys && b is PanelKeys bKeys ? aKeys.Keys.SequenceEqual(bKeys.Keys) : a == b;

    private static bool ValidFace(IReadOnlyList<PanelPoint> face) =>
        face.Count is >= 3 and <= 32 && face.All(OnBoard) && Area(face) > Epsilon &&
        Edges(face).All(edge => edge.Length > Epsilon && face.All(point => edge.Side(point) >= -Epsilon))
        && face.Select((point, i) => face.Take(i).All(other => new PanelEdge(other, point).Length > Epsilon)).All(valid => valid);

    private static double Area(IReadOnlyList<PanelPoint> face) =>
        Edges(face).Sum(edge => edge.Start.X * edge.End.Y - edge.Start.Y * edge.End.X);

    private static bool Overlaps(IReadOnlyList<PanelPoint> a, IReadOnlyList<PanelPoint> b) =>
        !Edges(a).Any(edge => b.All(point => edge.Side(point) <= Epsilon))
        && !Edges(b).Any(edge => a.All(point => edge.Side(point) <= Epsilon));

    private static PanelEdit Validated(PanelGeometry current, IEnumerable<PanelRegion> replacement)
    {
        PanelGeometry candidate = current with { Regions = replacement.ToArray() };
        return Validate(candidate) is { } error ? new(null, error) : new(candidate, null);
    }

    public static PanelEdit CutLine(PanelGeometry geometry, PanelEdge line, string? selected, string emptyName)
    {
        if (line.Length < .001) return new(null, "Panel_InvalidCut");
        bool empty = geometry.Regions.Count == 0 && selected is null;
        IEnumerable<PanelRegion> regions = empty ? [Empty(emptyName, [Rectangle(0, 0, 1, 1)])] : geometry.Regions;
        List<PanelRegion> result = [];
        bool changed = false;
        foreach (PanelRegion region in regions)
        {
            if (selected is null || selected == region.Id)
            {
                var halves = Split(region.Faces, line);
                if (halves.Left.Count != 0 && halves.Right.Count != 0)
                {
                    result.Add(region with { Faces = halves.Left });
                    result.Add(Empty(emptyName, halves.Right));
                    changed = true;
                    continue;
                }
            }
            result.Add(region);
        }
        return changed ? Validated(geometry, result) : new(null, "Panel_InvalidCut");
    }

    public static PanelEdit CutRectangle(PanelGeometry geometry, PanelPoint a, PanelPoint b, string? selected, string emptyName)
    {
        var box = Rectangle(Math.Min(a.X, b.X), Math.Min(a.Y, b.Y), Math.Max(a.X, b.X), Math.Max(a.Y, b.Y));
        if (!ValidFace(box)) return new(null, "Panel_InvalidCut");
        if (selected is null) return Validated(geometry, [.. geometry.Regions, Empty(emptyName, [box])]);
        PanelRegion? region = geometry.Regions.FirstOrDefault(item => item.Id == selected);
        if (region is null) return new(null, "Panel_InvalidCut");
        var (inside, outside) = Partition(region.Faces, box);
        if (inside.Count == 0 || outside.Count == 0) return new(null, "Panel_InvalidCut");
        return Validated(geometry, [.. geometry.Regions.Select(item => item.Id == selected ? region with { Faces = outside } : item),
            Empty(emptyName, inside)]);
    }

    public static PanelEdit Merge(PanelGeometry geometry, IReadOnlySet<string> ids, string inheritId)
    {
        PanelRegion[] selected = geometry.Regions.Where(region => ids.Contains(region.Id)).ToArray();
        PanelRegion? inherit = selected.FirstOrDefault(region => region.Id == inheritId);
        if (selected.Length < 2 || inherit is null) return new(null, "Panel_InvalidMerge");
        var seams = new PanelSurface(geometry).Seams;
        HashSet<string> connected = new(StringComparer.Ordinal) { inheritId };
        for (int i = 1; i < selected.Length; i++)
        {
            foreach (PanelSeam seam in seams.Where(seam => ids.Contains(seam.First) && ids.Contains(seam.Second)))
            {
                if (connected.Contains(seam.First)) connected.Add(seam.Second);
                if (connected.Contains(seam.Second)) connected.Add(seam.First);
            }
        }
        if (connected.Count != selected.Length) return new(null, "Panel_InvalidMerge");
        return Validated(geometry, geometry.Regions
            .Where(region => !ids.Contains(region.Id) || region.Id == inheritId)
            .Select(region => region.Id == inheritId ? inherit with { Faces = selected.SelectMany(item => item.Faces).ToArray() } : region));
    }

    public static PanelEdit MoveEdge(PanelGeometry geometry, PanelSeam seam, PanelPoint delta)
    {
        PanelRegion a = geometry.Regions.Single(region => region.Id == seam.First);
        PanelRegion b = geometry.Regions.Single(region => region.Id == seam.Second);
        PanelEdge prior = seam.Edge;
        bool aOnLeft = a.Faces.SelectMany(face => face).All(point => prior.Side(point) >= -Epsilon);
        bool bOnOtherSide = b.Faces.SelectMany(face => face).All(point => aOnLeft ? prior.Side(point) <= Epsilon : prior.Side(point) >= -Epsilon);
        bool aOnOneSide = aOnLeft || a.Faces.SelectMany(face => face).All(point => prior.Side(point) <= Epsilon);
        double offset = prior.Side(prior.Start + delta);
        if (Math.Abs(offset) <= Epsilon) return new(null, null);
        if (!aOnOneSide || !bOnOtherSide)
        {
            // A concave neighbor can wrap around the plane of the selected
            // edge. Transfer only the swept strip beside that edge.
            var normal = new PanelPoint(prior.Start.Y - prior.End.Y, prior.End.X - prior.Start.X);
            PanelPoint shift = normal * (offset / (prior.Length * prior.Length));
            PanelPoint[] sweep = [prior.Start, prior.End, prior.End + shift, prior.Start + shift];
            if (offset < 0) Array.Reverse(sweep);
            PanelRegion donor = offset > 0 ? a : b;
            PanelRegion receiver = offset > 0 ? b : a;
            var (transfer, remainder) = Partition(donor.Faces, sweep);
            if (transfer.Count == 0 || remainder.Count == 0)
                return new(null, "Panel_InvalidEdgeMove");
            return Validated(geometry, geometry.Regions.Select(region =>
                region.Id == donor.Id ? donor with { Faces = remainder }
                : region.Id == receiver.Id ? receiver with { Faces = [.. receiver.Faces, .. transfer] }
                : region));
        }
        var split = Split([.. a.Faces, .. b.Faces], new(prior.Start + delta, prior.End + delta));
        if (split.Left.Count == 0 || split.Right.Count == 0) return new(null, "Panel_InvalidEdgeMove");
        return Validated(geometry, geometry.Regions.Select(region => region.Id == a.Id
            ? a with { Faces = aOnLeft ? split.Left : split.Right }
            : region.Id == b.Id ? b with { Faces = aOnLeft ? split.Right : split.Left } : region));
    }

    public static PanelEdit ResizeRectangle(PanelGeometry geometry, PanelRectangleEdge handle, PanelPoint delta)
    {
        PanelRegion? region = geometry.Regions.FirstOrDefault(item => item.Id == handle.RegionId);
        if (region is null || RectangleOutline(region) is not { } rectangle || !Edges(rectangle).Contains(handle.Edge))
            return new(null, "Panel_InvalidPolygon");

        bool vertical = handle.IsVertical;
        double offset = vertical ? delta.X : delta.Y;
        if (Math.Abs(offset) <= Epsilon) return new(null, null);
        double position = vertical ? handle.Edge.Start.X : handle.Edge.Start.Y;
        PanelPoint[] face = rectangle.Select(point => vertical
            ? point.X == position ? point with { X = point.X + offset } : point
            : point.Y == position ? point with { Y = point.Y + offset } : point).ToArray();
        return Replace(geometry, region with { Faces = [face] });
    }

    public static PanelEdit MoveRegion(PanelGeometry geometry, PanelSurface surface, string sourceId, PanelPoint start, PanelPoint end, string emptyName)
    {
        PanelRegion source = geometry.Regions.Single(region => region.Id == sourceId);
        PanelHit hit = surface.Hit(end);
        if (!OnBoard(end) || hit.Gap || hit.RegionId == sourceId) return new(null, null);
        if (hit.RegionId is { } targetId)
        {
            PanelRegion target = geometry.Regions.Single(region => region.Id == targetId);
            return Validated(geometry, geometry.Regions.Select(region => region.Id == targetId
                ? target with { Name = source.Name, Action = source.Action }
                : region.Id == sourceId ? source with { Name = target.Action is null ? emptyName : target.Name, Action = target.Action } : region));
        }
        PanelPoint delta = end - start;
        return Validated(geometry, geometry.Regions.Select(region => region.Id == sourceId
            ? source with { Faces = source.Faces.Select(face => (IReadOnlyList<PanelPoint>)face.Select(point => point + delta).ToArray()).ToArray() } : region));
    }

    public static PanelEdit Delete(PanelGeometry geometry, IReadOnlySet<string> ids) =>
        ids.Count == 0 ? new(null, null) : Validated(geometry, geometry.Regions.Where(region => !ids.Contains(region.Id)));

    public static PanelEdit Replace(PanelGeometry geometry, PanelRegion edited) =>
        Validated(geometry, geometry.Regions.Select(region => region.Id == edited.Id ? edited : region));

    private static PanelRegion Empty(string name, IReadOnlyList<IReadOnlyList<PanelPoint>> faces) =>
        new(PanelLayout.NewId(), name, null, faces);

    private static (IReadOnlyList<IReadOnlyList<PanelPoint>> Inside, List<IReadOnlyList<PanelPoint>> Outside)
        Partition(IReadOnlyList<IReadOnlyList<PanelPoint>> faces, IReadOnlyList<PanelPoint> outline)
    {
        List<IReadOnlyList<PanelPoint>> outside = [];
        IReadOnlyList<IReadOnlyList<PanelPoint>> inside = faces;
        foreach (PanelEdge edge in Edges(outline))
        {
            var halves = Split(inside, edge);
            inside = halves.Left;
            outside.AddRange(halves.Right);
        }
        return (inside, outside);
    }

    private static (List<IReadOnlyList<PanelPoint>> Left, List<IReadOnlyList<PanelPoint>> Right)
        Split(IReadOnlyList<IReadOnlyList<PanelPoint>> faces, PanelEdge line)
    {
        List<IReadOnlyList<PanelPoint>> left = [];
        List<IReadOnlyList<PanelPoint>> right = [];
        foreach (var face in faces)
        {
            if (Clip(face, line, 1) is { } positive) left.Add(positive);
            if (Clip(face, line, -1) is { } negative) right.Add(negative);
        }
        return (left, right);
    }

    private static IReadOnlyList<PanelPoint>? Clip(IReadOnlyList<PanelPoint> face, PanelEdge line, int side)
    {
        List<PanelPoint> result = [];
        void Add(PanelPoint point)
        {
            if (result.Count == 0 || new PanelEdge(result[^1], point).Length > Epsilon) result.Add(point);
        }
        foreach (PanelEdge edge in Edges(face))
        {
            double a = side * line.Side(edge.Start);
            double b = side * line.Side(edge.End);
            if (a >= -Epsilon) Add(edge.Start);
            if ((a > Epsilon && b < -Epsilon) || (a < -Epsilon && b > Epsilon)) Add(edge.At(a / (a - b)));
        }
        if (result.Count > 1 && new PanelEdge(result[0], result[^1]).Length <= Epsilon) result.RemoveAt(result.Count - 1);
        return result.Count >= 3 && Area(result) > Epsilon ? result : null;
    }
}

public sealed class PanelHistory
{
    private readonly PanelGeometry _original;
    private readonly Stack<PanelGeometry> _undo = new();
    private readonly Stack<PanelGeometry> _redo = new();
    public PanelHistory(PanelGeometry original)
    {
        _original = original;
        Current = original;
    }

    public PanelGeometry Current { get; private set; }
    public bool IsDirty => !PanelGeometryEditor.ContentEquals(Current, _original);
    public bool CanUndo => _undo.Count != 0;
    public bool CanRedo => _redo.Count != 0;

    public void Commit(PanelEdit edit)
    {
        if (edit.Geometry is not { } geometry || PanelGeometryEditor.ContentEquals(Current, geometry)) return;
        _undo.Push(Current);
        Current = geometry;
        _redo.Clear();
    }

    public void Undo()
    {
        if (!_undo.TryPop(out PanelGeometry? prior)) return;
        _redo.Push(Current);
        Current = prior;
    }

    public void Redo()
    {
        if (!_redo.TryPop(out PanelGeometry? next)) return;
        _undo.Push(Current);
        Current = next;
    }
}
