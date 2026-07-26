using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service.Validation;

namespace Hermes.Mgmt.Service.Tests;

public class DnsNameTests
{
    [Theory]
    [InlineData("Example.COM", "example.com")]
    [InlineData("example.com.", "example.com")]
    [InlineData("  example.com  ", "example.com")]
    [InlineData("münchen.de", "xn--mnchen-3ya.de")]
    public void Normalize_canonicalizes(string input, string expected) =>
        Assert.Equal(expected, DnsName.Normalize(input));

    [Theory]
    [InlineData("@", "example.com")]
    [InlineData("", "example.com")]
    [InlineData("www", "www.example.com")]
    [InlineData("WWW", "www.example.com")]
    [InlineData("www.example.com", "www.example.com")]
    [InlineData("www.example.com.", "www.example.com")]
    [InlineData("example.com", "example.com")]
    [InlineData("a.b", "a.b.example.com")]
    public void ToFqdn_expands_relative_names(string input, string expected) =>
        Assert.Equal(expected, DnsName.ToFqdn(input, "example.com"));

    [Theory]
    [InlineData("example.com", true)]
    [InlineData("_dmarc.example.com", true)]        // underscore labels are required in practice
    [InlineData("*.example.com", true)]             // wildcard, left-most only
    [InlineData("a.*.example.com", false)]
    [InlineData("-bad.example.com", false)]
    [InlineData("bad-.example.com", false)]
    [InlineData("", false)]
    public void IsValidHostname_matches_dns_rules(string name, bool expected) =>
        Assert.Equal(expected, DnsName.IsValidHostname(name));

    [Fact]
    public void IsValidHostname_rejects_an_over_long_label()
    {
        var label = new string('a', 64);
        Assert.False(DnsName.IsValidHostname($"{label}.example.com"));
    }

    [Theory]
    [InlineData("192.0.2.1", true)]
    [InlineData("255.255.255.255", true)]
    [InlineData("256.0.0.1", false)]
    // IPAddress.TryParse accepts these shorthand forms, but Hermes's parser does not — so the
    // API must not either, or the zone would fail validation on the DNS side instead.
    [InlineData("10.1", false)]
    [InlineData("2001:db8::1", false)]
    public void IsValidIpv4_is_strict(string value, bool expected) =>
        Assert.Equal(expected, DnsName.IsValidIpv4(value));

    [Theory]
    [InlineData("2001:db8::1", true)]
    [InlineData("::1", true)]
    [InlineData("192.0.2.1", false)]
    [InlineData("not-an-address", false)]
    public void IsValidIpv6_accepts_only_v6(string value, bool expected) =>
        Assert.Equal(expected, DnsName.IsValidIpv6(value));
}

public class DnsValidatorTests
{
    private const string Apex = "example.com";

    private static RecordDraft Draft(
        RecordType type, string name = Apex, string? data = null, Action<RecordDraft>? _ = null) =>
        new() { Name = name, Type = type, Ttl = 300, Data = data };

    [Fact]
    public void A_record_requires_a_valid_ipv4()
    {
        var errors = DnsValidator.Validate(Draft(RecordType.A, data: "not-an-ip"), Apex);
        Assert.Equal("Not a valid IPv4 address.", Assert.Contains("data", errors)[0]);
    }

    [Fact]
    public void A_record_with_a_valid_address_passes() =>
        Assert.Empty(DnsValidator.Validate(Draft(RecordType.A, data: "192.0.2.1"), Apex));

    [Fact]
    public void Record_outside_the_zone_is_rejected()
    {
        var errors = DnsValidator.Validate(
            Draft(RecordType.A, name: "www.other.com", data: "192.0.2.1"), Apex);

        Assert.Equal($"Must be inside {Apex}.", Assert.Contains("name", errors)[0]);
    }

    [Fact]
    public void Mx_without_a_preference_is_rejected()
    {
        var errors = DnsValidator.Validate(
            Draft(RecordType.MX, data: "mail.example.com"), Apex);

        Assert.Contains("preference", errors);
    }

    [Fact]
    public void Srv_requires_priority_weight_and_port()
    {
        var errors = DnsValidator.Validate(
            Draft(RecordType.SRV, name: "_sip._tcp.example.com", data: "sip.example.com"), Apex);

        Assert.Contains("priority", errors);
        Assert.Contains("weight", errors);
        Assert.Contains("port", errors);
    }

    [Fact]
    public void Caa_rejects_an_unknown_tag()
    {
        var draft = Draft(RecordType.CAA, data: "letsencrypt.org") with { Tag = "bogus" };
        Assert.Contains("tag", DnsValidator.Validate(draft, Apex));
    }

    [Fact]
    public void Txt_may_be_empty()
    {
        // Everything else requires data; TXT is the exception, so the blanket "required" check
        // must not fire for it.
        var errors = DnsValidator.Validate(Draft(RecordType.TXT, data: ""), Apex);
        Assert.Empty(errors);
    }

    [Fact]
    public void Alias_is_rejected_below_the_apex()
    {
        var errors = DnsValidator.Validate(
            Draft(RecordType.ALIAS, name: "www.example.com", data: "target.example.net"), Apex);

        Assert.Contains("name", errors);
    }

    [Fact]
    public void NormalizeType_flattens_a_cname_at_the_apex() =>
        Assert.Equal(RecordType.ALIAS, DnsValidator.NormalizeType(RecordType.CNAME, Apex, Apex));

    [Fact]
    public void NormalizeType_leaves_a_cname_below_the_apex_alone() =>
        Assert.Equal(
            RecordType.CNAME,
            DnsValidator.NormalizeType(RecordType.CNAME, "www.example.com", Apex));

    [Fact]
    public void Alias_is_not_client_submittable() =>
        Assert.False(DnsValidator.IsClientSubmittable(RecordType.ALIAS));

    [Fact]
    public void Cname_conflicts_with_an_existing_record_at_the_same_name()
    {
        IReadOnlyList<DnsRecord> existing =
        [
            new()
            {
                Id = "r1", ZoneId = "z", Name = "www.example.com",
                Type = RecordType.A, Ttl = 300, Data = "192.0.2.1"
            }
        ];

        var conflicts = DnsValidator.ValidateAgainstZone(
            Draft(RecordType.CNAME, name: "www.example.com", data: "example.com"), existing);

        Assert.Contains("name", conflicts);
    }

    [Fact]
    public void A_record_conflicts_with_an_existing_cname_at_the_same_name()
    {
        // The other direction of RFC 1034 §3.6.2 — easy to implement only one way round.
        IReadOnlyList<DnsRecord> existing =
        [
            new()
            {
                Id = "r1", ZoneId = "z", Name = "www.example.com",
                Type = RecordType.CNAME, Ttl = 300, Data = "example.com"
            }
        ];

        var conflicts = DnsValidator.ValidateAgainstZone(
            Draft(RecordType.A, name: "www.example.com", data: "192.0.2.1"), existing);

        Assert.Contains("name", conflicts);
    }

    [Fact]
    public void Two_a_records_at_the_same_name_are_allowed()
    {
        IReadOnlyList<DnsRecord> existing =
        [
            new()
            {
                Id = "r1", ZoneId = "z", Name = "www.example.com",
                Type = RecordType.A, Ttl = 300, Data = "192.0.2.1"
            }
        ];

        var conflicts = DnsValidator.ValidateAgainstZone(
            Draft(RecordType.A, name: "www.example.com", data: "192.0.2.2"), existing);

        Assert.Empty(conflicts);
    }

    [Fact]
    public void An_identical_record_is_a_duplicate()
    {
        IReadOnlyList<DnsRecord> existing =
        [
            new()
            {
                Id = "r1", ZoneId = "z", Name = "www.example.com",
                Type = RecordType.A, Ttl = 300, Data = "192.0.2.1"
            }
        ];

        var conflicts = DnsValidator.ValidateAgainstZone(
            Draft(RecordType.A, name: "www.example.com", data: "192.0.2.1"), existing);

        Assert.Contains("data", conflicts);
    }

    [Fact]
    public void Soa_expire_must_exceed_refresh()
    {
        var soa = new Soa
        {
            Mname = "ns1.hermes-dns.net",
            Rname = "hostmaster.example.com",
            Refresh = 3600,
            Retry = 600,
            Expire = 3600,
            Minimum = 60,
            Ttl = 3600
        };

        Assert.Contains("expire", DnsValidator.ValidateSoa(soa));
    }

    [Fact]
    public void A_sensible_soa_passes()
    {
        var soa = new Soa
        {
            Mname = "ns1.hermes-dns.net",
            Rname = "hostmaster.example.com",
            Refresh = 3600,
            Retry = 600,
            Expire = 604800,
            Minimum = 60,
            Ttl = 3600
        };

        Assert.Empty(DnsValidator.ValidateSoa(soa));
    }
}
