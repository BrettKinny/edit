// Single-line comment
/* Block comment
   spanning multiple lines */

global using System.Text;
using System;
using System.Collections.Generic;

#nullable enable
#if DEBUG
#pragma warning disable CS0168
#endif

namespace Highlighting.Sample;

[assembly: CLSCompliant(true)]
[Obsolete("Use NewGreeter instead")]
public sealed class Greeter
{
    private const decimal TaxRate = 0.125m;
    private readonly string _name;

    public Greeter(string name)
    {
        _name = name;
    }

    public string SayHello(int count = 3)
    {
        var integers = new List<int> { 0, 1_000, -42, 42u, 42L, 42ul };
        var floatingPoint = new[] { .5f, 3.14159, 6.022e23, 1.5e-3D };
        var hex = 0xDEAD_BEEFul;
        var binary = 0b1010_0110U;

        // Regular, character, verbatim, and UTF-8 strings.
        var regular = "Hello, \"world\"\n";
        var character = '\u263A';
        var verbatim = @"C:\Temp\file.txt and ""quoted""";
        ReadOnlySpan<byte> utf8 = "hello"u8;

        // Interpolation, alignment, formatting, and escaped braces.
        var interpolated = $"Hello {_name}, count={count}";
        var formatted = $"Name={_name,-10} Count={count:D2}";
        var interpolatedVerbatim = $@"Name={_name}\nCount={count}";
        var alternatePrefix = @$"Name={_name}";
        var literalBraces = $"Literal braces: {{ and }}";

        // Raw and interpolated raw strings, including multiline forms.
        var raw = """He said "hello".""";
        var multilineRaw = """
            first line
            second "quoted" line
            """;
        var interpolatedRaw = $"""Hello {_name}; JSON: {{ "count": {count} }}""";
        var doubleDollarRaw = $$"""
            Literal braces: {value}
            Interpolation: {{count}}
            """;

        // This is indexing, not an attribute.
        var first = integers[Count()];
        return count > 0 ? interpolated : string.Empty;
    }

    private static int Count() => 0;
}

public record Person(string Name, int Age);

public static class Program
{
    public static int Main(string[] args)
    {
        bool enabled = true;
        object? missing = null;
        var disabled = false;

        // Keyword boundary regressions: `int` must not match `internal`,
        // and these ordinary identifiers must not be partially highlighted.
        internalState = 1;
        printable = enabled;
        sprint = disabled;

        // Generic type parameters and verbatim identifiers are identifiers.
        T Identity<T>(T item) => item;
        var @class = Identity(42);

        if (args.Length == 0)
        {
            return 1;
        }

        try
        {
            var greeter = new Greeter(args[0]);
            Console.WriteLine(greeter.SayHello());
        }
        catch (Exception exception) when (exception.Message.Length > 0)
        {
            throw;
        }
        finally
        {
            Console.WriteLine(nameof(Program));
        }

        return @class;
    }

    private static int internalState;
    private static bool printable;
    private static bool sprint;
}
