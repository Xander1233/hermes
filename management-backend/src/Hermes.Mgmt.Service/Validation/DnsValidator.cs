using Hermes.Mgmt.Common.Domain;

namespace Hermes.Mgmt.Service.Validation;

public sealed record RecordDraft
{
    public required string Name { get; init; }
    public RecordType Type { get; init; }
    public uint Ttl { get; init; }
    public string? Data { get; init; }
    public ushort? Preference { get; init; }
    public ushort? Priority { get; init; }
    public ushort? Weight { get; init; }
    public ushort? Port { get; init; }
    public byte? Flags { get; init; }
    public string? Tag { get; init; }
    public string? Notes { get; init; }
}

public static class DnsValidator
{
    public const uint MaxTtl = 604800;
    public const int MaxNotesLength = 1000;

    private static readonly string[] CaaTags = ["issue", "issuewild", "iodef"];

    public static bool IsClientSubmittable(RecordType type) => type != RecordType.ALIAS;

    public static bool TryParseType(string? value, out RecordType type) => Enum.TryParse(value?.Trim(), ignoreCase: true, out type) && Enum.IsDefined(type);

    public static RecordType NormalizeType(RecordType type, string name, string apex) => type == RecordType.CNAME && name == apex ? RecordType.ALIAS : type;

    public static Dictionary<string, string[]> Validate(RecordDraft record, string apex)
    {
        var errors = new Dictionary<string, string[]>();
        void Fail(string field, string message) => errors[field] = [message];

        var data = record.Data?.Trim() ?? string.Empty;

        if (!DnsName.IsValidHostname(record.Name))
        {
            Fail("name", "Not a valid host name.");
        }
        else if (!DnsName.IsInZone(record.Name, apex))
        {
            Fail("name", $"Must be inside {apex}.");
        }

        if (record.Ttl > MaxTtl)
        {
            Fail("ttl", $"TTL must be between 0 and {MaxTtl} seconds.");
        }

        if (record.Notes is { Length: > MaxNotesLength })
        {
            Fail("notes", $"Notes must be at most {MaxNotesLength} characters.");
        }

        if (data.Length == 0 && record.Type != RecordType.TXT)
        {
            Fail("data", "A value is required.");
            return errors;
        }

        switch (record.Type)
        {
            case RecordType.A:
                if (!DnsName.IsValidIpv4(data)) Fail("data", "Not a valid IPv4 address.");
                break;

            case RecordType.AAAA:
                if (!DnsName.IsValidIpv6(data)) Fail("data", "Not a valid IPv6 address.");
                break;

            case RecordType.CNAME:
                if (!DnsName.IsValidHostname(data)) Fail("data", "Not a valid target name.");
                if (record.Name == apex)
                    Fail("name", "A CNAME cannot live at the apex; it is stored as a flattened ALIAS.");
                break;

            case RecordType.ALIAS:
                if (!DnsName.IsValidHostname(data)) Fail("data", "Not a valid target name.");
                if (record.Name != apex)
                    Fail("name", "ALIAS is only valid at the zone apex.");
                break;

            case RecordType.NS:
            case RecordType.PTR:
                if (!DnsName.IsValidHostname(data)) Fail("data", "Not a valid host name.");
                break;

            case RecordType.MX:
                if (!DnsName.IsValidHostname(data)) Fail("data", "Not a valid mail server name.");
                if (record.Preference is null) Fail("preference", "Priority is required for MX records.");
                break;

            case RecordType.SRV:
                if (!DnsName.IsValidHostname(data)) Fail("data", "Not a valid target name.");
                if (record.Priority is null) Fail("priority", "Priority is required for SRV records.");
                if (record.Weight is null) Fail("weight", "Weight is required for SRV records.");
                if (record.Port is null or 0) Fail("port", "Port must be between 1 and 65535.");
                break;

            case RecordType.CAA:
                if (string.IsNullOrWhiteSpace(record.Tag))
                    Fail("tag", "Tag is required for CAA records.");
                else if (!CaaTags.Contains(record.Tag.Trim().ToLowerInvariant()))
                    Fail("tag", $"Tag must be one of: {string.Join(", ", CaaTags)}.");
                break;

            case RecordType.TXT:
                if (data.Length > 65535) Fail("data", "Content is too long.");
                break;
        }

        return errors;
    }

    public static Dictionary<string, string[]> ValidateAgainstZone(RecordDraft candidate, IReadOnlyList<DnsRecord> existing)
    {
        var errors = new Dictionary<string, string[]>();
        var siblings = existing.Where(r => r.Name == candidate.Name).ToList();

        var candidateIsAlias = candidate.Type is RecordType.CNAME or RecordType.ALIAS;
        var conflicts = siblings.Any(r => candidateIsAlias || r.Type is RecordType.CNAME or RecordType.ALIAS);

        if (conflicts)
        {
            errors["name"] =
            [
                $"{candidate.Name} already has records of another type; " +
                "a CNAME/ALIAS must be the only record at its name."
            ];
            return errors;
        }

        if (siblings.Any(r => r.Type == candidate.Type && r.Data == candidate.Data))
        {
            errors["data"] = ["An identical record already exists."];
        }

        return errors;
    }

    public static Dictionary<string, string[]> ValidateSoa(Soa soa)
    {
        var errors = new Dictionary<string, string[]>();
        void Fail(string field, string message) => errors[field] = [message];

        if (!DnsName.IsValidHostname(soa.Mname)) Fail("mname", "Primary nameserver must be a valid host name.");

        if (!DnsName.IsValidHostname(soa.Rname)) Fail("rname", "Responsible party must be a valid name, e.g. hostmaster.example.com.");

        if (soa.Refresh <= 0) Fail("refresh", "Must be greater than zero.");
        if (soa.Retry <= 0) Fail("retry", "Must be greater than zero.");

        if (soa.Expire <= soa.Refresh) Fail("expire", "Expire must be greater than refresh.");

        if (soa.Minimum > MaxTtl) Fail("minimum", $"Must be at most {MaxTtl} seconds.");
        if (soa.Ttl > MaxTtl) Fail("ttl", $"Must be at most {MaxTtl} seconds.");

        return errors;
    }
}
