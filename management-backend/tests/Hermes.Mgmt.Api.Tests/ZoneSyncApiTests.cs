using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Hermes.Mgmt.Common.Contracts.Sync.V1;
using Hermes.Mgmt.Common.Contracts.V1;

namespace Hermes.Mgmt.Api.Tests;

/// <summary>
/// Tests for the contract the Hermes DNS fleet pulls.
/// </summary>
/// <remarks>
/// These assert the JSON shape by name rather than only through the C# DTOs, because the actual
/// consumer is Rust serde: a renamed property compiles fine on this side and fails silently on
/// the other, taking a zone — or the whole manifest — out of service.
/// </remarks>
public class ZoneSyncApiTests(ApiFactory factory) : IClassFixture<ApiFactory>
{
    private const string SyncUrl = "/sync/v1/zones";
    private const string DashboardUrl = "/api/v1";

    [Fact]
    public async Task The_sync_api_rejects_a_request_with_no_token()
    {
        var response = await factory.CreateSyncClient(token: null).GetAsync(SyncUrl);
        Assert.Equal(HttpStatusCode.Unauthorized, response.StatusCode);
    }

    [Fact]
    public async Task The_sync_api_rejects_a_wrong_token()
    {
        var response = await factory.CreateSyncClient("not-the-token").GetAsync(SyncUrl);
        Assert.Equal(HttpStatusCode.Unauthorized, response.StatusCode);
    }

    [Fact]
    public async Task A_dashboard_user_token_does_not_open_the_sync_api()
    {
        // The two schemes are separate for a reason: the sync API exposes every tenant's
        // zones, so a user credential must not reach it.
        var response = await factory.CreateUserClient("user-sync-probe").GetAsync(SyncUrl);
        Assert.Equal(HttpStatusCode.Unauthorized, response.StatusCode);
    }

    [Fact]
    public async Task A_sync_token_does_not_open_the_dashboard_api()
    {
        var response = await factory.CreateSyncClient().GetAsync($"{DashboardUrl}/zones");
        Assert.Equal(HttpStatusCode.Unauthorized, response.StatusCode);
    }

    [Fact]
    public async Task The_manifest_spans_every_tenant()
    {
        // Hermes deletes any zone it tracks that the manifest omits, so filtering this by user
        // would unpublish other tenants' zones.
        await CreateZoneAsync("user-a", "tenant-a.example");
        await CreateZoneAsync("user-b", "tenant-b.example");

        var manifest = await factory.CreateSyncClient()
            .GetFromJsonAsync<ZoneManifestResponse>(SyncUrl);

        Assert.Contains(manifest!.Zones, z => z.Apex == "tenant-a.example");
        Assert.Contains(manifest.Zones, z => z.Apex == "tenant-b.example");
    }

    [Fact]
    public async Task The_manifest_uses_the_property_names_hermes_deserializes()
    {
        await CreateZoneAsync("user-manifest-shape", "shape.example");

        var json = await factory.CreateSyncClient().GetStringAsync(SyncUrl);
        using var document = JsonDocument.Parse(json);

        Assert.True(document.RootElement.TryGetProperty("version", out _));
        Assert.True(document.RootElement.TryGetProperty("zones", out var zones));

        var entry = zones.EnumerateArray().First();
        Assert.True(entry.TryGetProperty("id", out _));
        Assert.True(entry.TryGetProperty("apex", out _));
        Assert.True(entry.TryGetProperty("version", out _));
    }

    [Fact]
    public async Task A_zone_document_uses_the_property_names_hermes_deserializes()
    {
        var zone = await CreateZoneAsync("user-doc-shape", "doc.example");
        await AddRecordAsync("user-doc-shape", zone.Id, new RecordRequest
        {
            Name = "@", Type = "MX", Ttl = 3600, Data = "mail.doc.example", Preference = 10
        });

        var json = await factory.CreateSyncClient().GetStringAsync($"{SyncUrl}/{zone.Id}");
        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;

        foreach (var field in new[] { "id", "apex", "version", "soa", "records" })
        {
            Assert.True(root.TryGetProperty(field, out _), $"missing '{field}'");
        }

        // SoaConfig in crates/config/src/dns.rs, field for field.
        var soa = root.GetProperty("soa");
        foreach (var field in new[]
                 { "mname", "rname", "serial", "refresh", "retry", "expire", "minimum", "ttl" })
        {
            Assert.True(soa.TryGetProperty(field, out _), $"missing soa.'{field}'");
        }

        // RecordConfig: `type` is the serde rename of `rtype`, and the MX preference must be
        // there or Hermes rejects the record.
        var record = root.GetProperty("records").EnumerateArray().First();
        Assert.Equal("MX", record.GetProperty("type").GetString());
        Assert.Equal(10, record.GetProperty("preference").GetInt32());
        Assert.True(record.TryGetProperty("name", out _));
        Assert.True(record.TryGetProperty("ttl", out _));
        Assert.True(record.TryGetProperty("data", out _));
    }

    [Fact]
    public async Task A_zone_document_never_leaks_notes()
    {
        // `notes` is user-authored management metadata. It is not DNS data and has no business
        // leaving the management plane.
        var zone = await CreateZoneAsync("user-notes", "annotated.example");
        await AddRecordAsync("user-notes", zone.Id, new RecordRequest
        {
            Name = "www",
            Type = "A",
            Ttl = 300,
            Data = "192.0.2.1",
            Notes = "SECRET-INTERNAL-NOTE"
        });

        var json = await factory.CreateSyncClient().GetStringAsync($"{SyncUrl}/{zone.Id}");
        Assert.DoesNotContain("SECRET-INTERNAL-NOTE", json);

        // Assert on the property rather than the raw text: a substring check for "notes" would
        // also match an apex or record value that happens to contain the word.
        using var document = JsonDocument.Parse(json);
        var record = document.RootElement.GetProperty("records").EnumerateArray().Single();

        Assert.False(record.TryGetProperty("notes", out _));
        Assert.False(record.TryGetProperty("id", out _));
        Assert.False(record.TryGetProperty("createdAt", out _));
    }

    [Fact]
    public async Task An_apex_cname_reaches_hermes_as_alias()
    {
        var zone = await CreateZoneAsync("user-sync-alias", "syncalias.example");
        await AddRecordAsync("user-sync-alias", zone.Id, new RecordRequest
        {
            Name = "@", Type = "CNAME", Ttl = 300, Data = "origin.hermes-edge.net"
        });

        var document = await factory.CreateSyncClient()
            .GetFromJsonAsync<ZoneDocumentResponse>($"{SyncUrl}/{zone.Id}");

        Assert.Equal("ALIAS", Assert.Single(document!.Records).Type);
    }

    [Fact]
    public async Task A_record_change_moves_the_zones_manifest_version()
    {
        // This is the whole propagation mechanism: Hermes refetches a zone only when its
        // version differs from the one it holds.
        var zone = await CreateZoneAsync("user-version", "version.example");
        var sync = factory.CreateSyncClient();

        var before = await ManifestVersionAsync(sync, zone.Id);

        await AddRecordAsync("user-version", zone.Id, new RecordRequest
        {
            Name = "www", Type = "A", Ttl = 300, Data = "192.0.2.1"
        });

        var after = await ManifestVersionAsync(sync, zone.Id);
        Assert.True(after > before, $"expected the version to move, got {before} then {after}");
    }

    [Fact]
    public async Task A_deleted_zone_simply_leaves_the_manifest()
    {
        // Deletion is implicit — there are no tombstones. Hermes removes whatever the manifest
        // stops listing.
        var zone = await CreateZoneAsync("user-sync-delete", "syncdelete.example");
        var sync = factory.CreateSyncClient();

        var manifest = await sync.GetFromJsonAsync<ZoneManifestResponse>(SyncUrl);
        Assert.Contains(manifest!.Zones, z => z.Id == zone.Id);

        await factory.CreateUserClient("user-sync-delete")
            .DeleteAsync($"{DashboardUrl}/zones/{zone.Id}");

        manifest = await sync.GetFromJsonAsync<ZoneManifestResponse>(SyncUrl);
        Assert.DoesNotContain(manifest!.Zones, z => z.Id == zone.Id);

        var document = await sync.GetAsync($"{SyncUrl}/{zone.Id}");
        Assert.Equal(HttpStatusCode.NotFound, document.StatusCode);
    }

    [Fact]
    public async Task A_pending_zone_is_published_with_its_records_before_delegation()
    {
        // The guarantee behind "users can add entries before the zone is delegated": those
        // records must already be at the DNS servers when the delegation lands. If publication
        // waited for activation, every zone would pass through a window where the parent points
        // at Hermes but Hermes has no zone, and every query in that window would fail.
        var zone = await CreateZoneAsync("user-pending-pub", "notyet.example");
        Assert.Equal("pending", zone.Status);

        await AddRecordAsync("user-pending-pub", zone.Id, new RecordRequest
        {
            Name = "www", Type = "A", Ttl = 300, Data = "192.0.2.1"
        });

        var sync = factory.CreateSyncClient();

        var manifest = await sync.GetFromJsonAsync<ZoneManifestResponse>(SyncUrl);
        Assert.Contains(manifest!.Zones, z => z.Id == zone.Id);

        var document = await sync.GetFromJsonAsync<ZoneDocumentResponse>($"{SyncUrl}/{zone.Id}");
        Assert.Equal("www.notyet.example", Assert.Single(document!.Records).Name);
    }

    [Fact]
    public async Task An_unknown_zone_is_a_404_so_hermes_can_isolate_it()
    {
        var response = await factory.CreateSyncClient().GetAsync($"{SyncUrl}/zone_does_not_exist");
        Assert.Equal(HttpStatusCode.NotFound, response.StatusCode);
    }

    private static async Task<ulong> ManifestVersionAsync(HttpClient sync, string zoneId)
    {
        var manifest = await sync.GetFromJsonAsync<ZoneManifestResponse>(SyncUrl);
        return manifest!.Zones.Single(z => z.Id == zoneId).Version;
    }

    private async Task<ZoneDetailResponse> CreateZoneAsync(string userId, string apex)
    {
        var response = await factory.CreateUserClient(userId)
            .PostAsJsonAsync($"{DashboardUrl}/zones", new CreateZoneRequest { Apex = apex });
        response.EnsureSuccessStatusCode();

        return (await response.Content.ReadFromJsonAsync<ZoneDetailResponse>())!;
    }

    private async Task AddRecordAsync(string userId, string zoneId, RecordRequest request)
    {
        var response = await factory.CreateUserClient(userId)
            .PostAsJsonAsync($"{DashboardUrl}/zones/{zoneId}/records", request);
        response.EnsureSuccessStatusCode();
    }
}
