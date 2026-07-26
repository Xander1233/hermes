using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service.Validation;
using Hermes.Mgmt.TestSupport;

namespace Hermes.Mgmt.Service.Tests;

public class ZoneSyncServiceTests
{
    [Fact]
    public async Task The_manifest_lists_every_zone_across_all_tenants()
    {
        // Not user-scoped, deliberately: Hermes deletes any zone it tracks that the manifest
        // omits, so a filtered manifest would unpublish other tenants' zones.
        var sut = new TestServices();
        sut.SeedZone("mine.com");
        sut.SeedZone("theirs.com", ownerId: sut.OtherCaller.UserId);

        var manifest = await sut.SyncService.GetManifestAsync();

        Assert.Equal(2, manifest.Zones.Count);
        Assert.Contains(manifest.Zones, z => z.Apex == "theirs.com");
    }

    [Fact]
    public async Task The_manifest_carries_the_per_zone_version()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        await sut.RecordService.CreateAsync(sut.Caller.UserId, zone.Id, new RecordDraft
        {
            Name = "www", Type = RecordType.A, Ttl = 300, Data = "192.0.2.1"
        });

        var manifest = await sut.SyncService.GetManifestAsync();
        var entry = Assert.Single(manifest.Zones);

        Assert.Equal(zone.Version + 1, entry.Version);
    }

    [Fact]
    public async Task The_advisory_version_moves_when_any_zone_changes()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var before = (await sut.SyncService.GetManifestAsync()).Version;
        await sut.RecordService.CreateAsync(sut.Caller.UserId, zone.Id, new RecordDraft
        {
            Name = "www", Type = RecordType.A, Ttl = 300, Data = "192.0.2.1"
        });
        var after = (await sut.SyncService.GetManifestAsync()).Version;

        Assert.NotEqual(before, after);
    }

    [Fact]
    public async Task A_zone_document_never_carries_notes()
    {
        // `notes` is management metadata, not DNS data — it is user-authored text that has no
        // business leaving the management plane.
        var sut = new TestServices();
        var zone = sut.SeedZone();

        await sut.RecordService.CreateAsync(sut.Caller.UserId, zone.Id, new RecordDraft
        {
            Name = "www",
            Type = RecordType.A,
            Ttl = 300,
            Data = "192.0.2.1",
            Notes = "internal-only note"
        });

        var document = await sut.SyncService.GetZoneDocumentAsync(zone.Id);

        var record = Assert.Single(document!.Records);
        var json = System.Text.Json.JsonSerializer.Serialize(
            record, new System.Text.Json.JsonSerializerOptions(System.Text.Json.JsonSerializerDefaults.Web));

        Assert.DoesNotContain("internal-only note", json);
        Assert.DoesNotContain("notes", json, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task A_zone_document_emits_the_type_names_hermes_parses()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        await sut.RecordService.CreateAsync(sut.Caller.UserId, zone.Id, new RecordDraft
        {
            Name = "@", Type = RecordType.CNAME, Ttl = 300, Data = "origin.hermes-edge.net"
        });

        var document = await sut.SyncService.GetZoneDocumentAsync(zone.Id);

        // Hermes's build_record_data matches on the uppercase name, and the apex CNAME must
        // reach it as ALIAS or the zone fails validation on the DNS side.
        Assert.Equal("ALIAS", Assert.Single(document!.Records).Type);
    }

    [Fact]
    public async Task A_zone_document_mirrors_the_soa_field_for_field()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var document = await sut.SyncService.GetZoneDocumentAsync(zone.Id);

        Assert.Equal(zone.Soa.Mname, document!.Soa.Mname);
        Assert.Equal(zone.Soa.Rname, document.Soa.Rname);
        Assert.Equal(zone.Soa.Serial.Value, document.Soa.Serial);
        Assert.Equal(zone.Soa.Refresh, document.Soa.Refresh);
        Assert.Equal(zone.Soa.Retry, document.Soa.Retry);
        Assert.Equal(zone.Soa.Expire, document.Soa.Expire);
        Assert.Equal(zone.Soa.Minimum, document.Soa.Minimum);
        Assert.Equal(zone.Soa.Ttl, document.Soa.Ttl);
    }

    [Fact]
    public async Task An_unknown_zone_returns_null_rather_than_throwing()
    {
        var sut = new TestServices();
        Assert.Null(await sut.SyncService.GetZoneDocumentAsync("zone_nope"));
    }

    [Fact]
    public async Task A_missing_zone_is_not_cached_negatively()
    {
        // Caching the miss would keep a freshly created zone invisible for the whole TTL —
        // exactly when propagation speed matters most.
        var sut = new TestServices();
        sut.Cache.ServeReads = true;

        Assert.Null(await sut.SyncService.GetZoneDocumentAsync("zone_later"));

        var zone = sut.SeedZone();
        Assert.NotNull(await sut.SyncService.GetZoneDocumentAsync(zone.Id));
    }
}
