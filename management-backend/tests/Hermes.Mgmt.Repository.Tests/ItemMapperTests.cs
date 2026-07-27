using Amazon.DynamoDBv2.Model;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Repository.DynamoDb;

namespace Hermes.Mgmt.Repository.Tests;

/// <summary>
/// Round-trip tests for the DynamoDB mapping.
/// </summary>
/// <remarks>
/// This is the layer that breaks silently: a field added to the domain but not to the mapper
/// compiles, deploys, and quietly stops persisting. Round-tripping every field is the cheapest
/// guard against that.
/// </remarks>
public class ItemMapperTests
{
    private static readonly DateTimeOffset Now = new(2026, 7, 21, 12, 30, 45, TimeSpan.Zero);

    private static Zone SampleZone() => new()
    {
        Id = "zone_abc",
        Apex = "example.com",
        OwnerId = "user-1",
        Status = ZoneStatus.Active,
        StatusMessage = null,
        Version = 42,
        Nameservers = ["ns1.hermes-dns.net", "ns2.hermes-dns.net"],
        RecordCount = 7,
        CreatedAt = Now,
        UpdatedAt = Now.AddHours(1),
        Soa = new Soa
        {
            Mname = "ns1.hermes-dns.net",
            Rname = "hostmaster.example.com",
            Serial = new Serial(2026072101),
            Refresh = 3600,
            Retry = 600,
            Expire = 604800,
            Minimum = 60,
            Ttl = 3600
        }
    };

    /// <summary>
    /// Compares two zones by value.
    /// </summary>
    /// <remarks>
    /// A record's generated equality compares <c>Nameservers</c> by reference, because
    /// <see cref="IReadOnlyList{T}"/> has no value equality — so a byte-perfect round trip
    /// still fails a plain <c>Assert.Equal</c>. The list is compared separately and then
    /// neutralized with a shared instance so the remaining fields can be compared as a whole.
    /// </remarks>
    private static void AssertSameZone(Zone expected, Zone actual)
    {
        Assert.Equal(expected.Nameservers, actual.Nameservers);

        IReadOnlyList<string> shared = [];
        Assert.Equal(expected with { Nameservers = shared }, actual with { Nameservers = shared });
    }

    [Fact]
    public void Zone_round_trips()
    {
        var original = SampleZone();
        AssertSameZone(original, ItemMapper.ToZone(ItemMapper.ToItem(original)));
    }

    [Fact]
    public void Zone_with_a_status_message_round_trips()
    {
        var original = SampleZone() with
        {
            Status = ZoneStatus.Error,
            StatusMessage = "Publish rejected: invalid MX target"
        };

        AssertSameZone(original, ItemMapper.ToZone(ItemMapper.ToItem(original)));
    }

    [Fact]
    public void Zone_keys_follow_the_documented_layout()
    {
        var item = ItemMapper.ToItem(SampleZone());

        Assert.Equal("ZONE#zone_abc", item[TableSchema.Pk].S);
        Assert.Equal(TableSchema.ZoneMetaSk, item[TableSchema.Sk].S);

        // GSI1 answers "zones owned by this user".
        Assert.Equal("USER#user-1", item[TableSchema.Gsi1Pk].S);

        // GSI2 answers "every zone", for the manifest, on a constant partition key.
        Assert.Equal(TableSchema.AllZonesPartition, item[TableSchema.Gsi2Pk].S);
        Assert.Equal("zone_abc", item[TableSchema.Gsi2Sk].S);
    }

    [Fact]
    public void A_pending_zone_carries_the_sparse_delegation_index_keys()
    {
        var due = Now.AddMinutes(2);
        var zone = SampleZone() with
        {
            Status = ZoneStatus.Pending,
            NextDelegationCheckAt = due
        };

        var item = ItemMapper.ToItem(zone);

        Assert.Equal(TableSchema.PendingDelegationPartition, item[TableSchema.Gsi3Pk].S);
        Assert.Equal(due, DateTimeOffset.Parse(item[TableSchema.Gsi3Sk].S));
    }

    [Fact]
    public void An_active_zone_is_absent_from_the_delegation_index()
    {
        // This is what makes the index sparse and self-cleaning. A zone write is a full
        // PutItem, so omitting these keys removes it from GSI3 — if they were ever written
        // unconditionally, the scanner's working set would grow with every zone ever created.
        var item = ItemMapper.ToItem(SampleZone() with { Status = ZoneStatus.Active });

        Assert.False(item.ContainsKey(TableSchema.Gsi3Pk));
        Assert.False(item.ContainsKey(TableSchema.Gsi3Sk));
    }

    [Fact]
    public void A_pending_zone_with_no_due_time_is_absent_from_the_delegation_index()
    {
        var item = ItemMapper.ToItem(SampleZone() with
        {
            Status = ZoneStatus.Pending,
            NextDelegationCheckAt = null
        });

        Assert.False(item.ContainsKey(TableSchema.Gsi3Pk));
    }

    [Fact]
    public void The_delegation_due_time_round_trips()
    {
        var zone = SampleZone() with
        {
            Status = ZoneStatus.Pending,
            NextDelegationCheckAt = Now.AddMinutes(15)
        };

        AssertSameZone(zone, ItemMapper.ToZone(ItemMapper.ToItem(zone)));
    }

    [Fact]
    public void The_delegation_index_sort_key_is_lexicographically_ordered_by_time()
    {
        // The scanner queries this as a range (`GSI3SK <= now`), which only works because the
        // ISO-8601 UTC form sorts the same way as the instants it encodes.
        var earlier = ItemMapper.ToItem(SampleZone() with
        {
            Status = ZoneStatus.Pending, NextDelegationCheckAt = Now
        })[TableSchema.Gsi3Sk].S;

        var later = ItemMapper.ToItem(SampleZone() with
        {
            Status = ZoneStatus.Pending, NextDelegationCheckAt = Now.AddHours(6)
        })[TableSchema.Gsi3Sk].S;

        Assert.True(string.CompareOrdinal(earlier, later) < 0);
    }

    [Fact]
    public void The_delegation_index_projects_everything_the_scanner_needs()
    {
        // The scanner reads whole zones off this index — apex to look up, nameservers to
        // compare, CreatedAt for the backoff — so a narrower projection would break it.
        var request = TableSchema.CreateTableRequest("t");
        var gsi3 = request.GlobalSecondaryIndexes.Single(i => i.IndexName == TableSchema.Gsi3);

        Assert.Equal(Amazon.DynamoDBv2.ProjectionType.ALL, gsi3.Projection.ProjectionType);
    }

    [Fact]
    public void The_manifest_index_projects_everything_ToZoneRef_reads()
    {
        // If these drift apart the manifest silently loses apexes or versions, and Hermes
        // deletes or fails to refresh zones as a result.
        var request = TableSchema.CreateTableRequest("t");
        var gsi2 = request.GlobalSecondaryIndexes.Single(i => i.IndexName == TableSchema.Gsi2);

        Assert.Contains(ItemAttributes.Apex, gsi2.Projection.NonKeyAttributes);
        Assert.Contains(ItemAttributes.Version, gsi2.Projection.NonKeyAttributes);
    }

    [Fact]
    public void ZoneRef_reads_from_a_projected_index_item()
    {
        // Only the projected attributes plus the index keys are present on a GSI2 read; the
        // mapper must not depend on anything else.
        var projected = new Dictionary<string, AttributeValue>
        {
            [TableSchema.Gsi2Pk] = new() { S = TableSchema.AllZonesPartition },
            [TableSchema.Gsi2Sk] = new() { S = "zone_abc" },
            [ItemAttributes.Apex] = new() { S = "example.com" },
            [ItemAttributes.Version] = new() { N = "42" }
        };

        var reference = ItemMapper.ToZoneRef(projected);

        Assert.Equal("zone_abc", reference.Id);
        Assert.Equal("example.com", reference.Apex);
        Assert.Equal(42ul, reference.Version);
    }

    [Fact]
    public void Record_round_trips_with_every_typed_field_populated()
    {
        var original = new DnsRecord
        {
            Id = "rec_1",
            ZoneId = "zone_abc",
            Name = "_sip._tcp.example.com",
            Type = RecordType.SRV,
            Ttl = 300,
            Data = "sip.example.com",
            Priority = 10,
            Weight = 20,
            Port = 5060,
            Notes = "voip",
            CreatedAt = Now,
            UpdatedAt = Now
        };

        Assert.Equal(original, ItemMapper.ToRecord(ItemMapper.ToItem(original)));
    }

    [Fact]
    public void Record_round_trips_with_the_optional_fields_null()
    {
        var original = new DnsRecord
        {
            Id = "rec_2",
            ZoneId = "zone_abc",
            Name = "example.com",
            Type = RecordType.ALIAS,
            Ttl = 300,
            Data = "origin.hermes-edge.net",
            CreatedAt = Now,
            UpdatedAt = Now
        };

        var restored = ItemMapper.ToRecord(ItemMapper.ToItem(original));

        Assert.Equal(original, restored);
        // Null must survive as null: an empty string would turn "no note" into "a blank note".
        Assert.Null(restored.Notes);
        Assert.Null(restored.Preference);
    }

    [Fact]
    public void A_caa_record_round_trips_its_flags_and_tag()
    {
        var original = new DnsRecord
        {
            Id = "rec_3",
            ZoneId = "zone_abc",
            Name = "example.com",
            Type = RecordType.CAA,
            Ttl = 3600,
            Data = "letsencrypt.org",
            Flags = 0,
            Tag = "issue",
            CreatedAt = Now,
            UpdatedAt = Now
        };

        var restored = ItemMapper.ToRecord(ItemMapper.ToItem(original));

        // Flags = 0 is meaningful and must not be conflated with "unset".
        Assert.Equal((byte)0, restored.Flags);
        Assert.Equal("issue", restored.Tag);
    }

    [Fact]
    public void Profile_round_trips()
    {
        var original = new UserProfile
        {
            Id = "user-1",
            Email = "user@example.com",
            DisplayName = "Test User",
            Company = null,
            Timezone = "Europe/Zurich",
            NotificationEmail = "ops@example.com",
            NotifyOnZoneErrors = true,
            CreatedAt = Now
        };

        Assert.Equal(original, ItemMapper.ToProfile(ItemMapper.ToItem(original)));
    }

    [Fact]
    public void Timestamps_survive_the_round_trip_to_the_second()
    {
        var restored = ItemMapper.ToZone(ItemMapper.ToItem(SampleZone()));
        Assert.Equal(Now, restored.CreatedAt);
    }

    [Fact]
    public void Nameservers_come_back_in_a_stable_order()
    {
        // DynamoDB string sets are unordered; the dashboard lists these, so the order the
        // mapper produces must not depend on what the service happened to return.
        var item = ItemMapper.ToItem(SampleZone());
        item[ItemAttributes.Nameservers].SS = ["ns2.hermes-dns.net", "ns1.hermes-dns.net"];

        var restored = ItemMapper.ToZone(item);

        Assert.Equal(["ns1.hermes-dns.net", "ns2.hermes-dns.net"], restored.Nameservers);
    }

    [Fact]
    public void A_missing_required_attribute_fails_loudly()
    {
        // Better a clear exception than a zone silently reconstructed with an empty apex.
        var item = ItemMapper.ToItem(SampleZone());
        item.Remove(ItemAttributes.Apex);

        Assert.Throws<InvalidOperationException>(() => ItemMapper.ToZone(item));
    }

    [Fact]
    public void The_apex_claim_is_keyed_by_apex_so_it_is_globally_unique()
    {
        var claim = ItemMapper.ToApexClaimItem("example.com", "zone_abc");

        Assert.Equal("APEX#example.com", claim[TableSchema.Pk].S);
        Assert.Equal(TableSchema.ApexClaimSk, claim[TableSchema.Sk].S);
    }
}
