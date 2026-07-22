using System.Globalization;
using System.Net;
using System.Net.Sockets;

namespace Hermes.Mgmt.Service.Validation;

public static class DnsName
{
    private static readonly IdnMapping Idn = new() { AllowUnassigned = false, UseStd3AsciiRules = false };

    public static string Normalize(string input)
    {
        var name = input.Trim().TrimEnd('.');
        if (name.Length == 0)
        {
            return string.Empty;
        }

        if (!name.All(char.IsAscii))
        {
            try
            {
                name = Idn.GetAscii(name);
            }
            catch (ArgumentException)
            { }
        }

        return name.ToLowerInvariant();
    }

    public static string ToFqdn(string input, string apex)
    {
        var name = Normalize(input);
        if (name.Length == 0 || name == "@")
        {
            return apex;
        }

        return name == apex || name.EndsWith("." + apex, StringComparison.Ordinal) ? name : $"{name}.{apex}";
    }

    public static bool IsValidHostname(string name)
    {
        if (string.IsNullOrEmpty(name) || name.Length > 253)
        {
            return false;
        }

        var labels = name.Split('.');
        for (var i = 0; i < labels.Length; i++)
        {
            if (i == 0 && labels[i] == "*")
            {
                continue;
            }

            if (!IsValidLabel(labels[i]))
            {
                return false;
            }
        }

        return true;
    }

    private static bool IsValidLabel(string label)
    {
        if (label.Length is 0 or > 63)
        {
            return false;
        }

        if (label[0] == '-' || label[^1] == '-')
        {
            return false;
        }

        return label.All(c => char.IsAsciiLetterOrDigit(c) || c == '-' || c == '_');
    }

    public static bool IsValidApex(string apex) => IsValidHostname(apex) && apex.Contains('.') && !apex.Contains('*');

    public static bool IsInZone(string name, string apex) => name == apex || name.EndsWith("." + apex, StringComparison.Ordinal);

    public static bool IsValidIpv4(string value) =>
        IPAddress.TryParse(value.Trim(), out var address)
        && address.AddressFamily == AddressFamily.InterNetwork
        && value.Trim().Count(c => c == '.') == 3;

    public static bool IsValidIpv6(string value) =>
        IPAddress.TryParse(value.Trim(), out var address)
        && address.AddressFamily == AddressFamily.InterNetworkV6;
}
