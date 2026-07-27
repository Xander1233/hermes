using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Hermes.Mgmt.Common.Contracts.V1;

namespace Hermes.Mgmt.Api.Tests;

/// <summary>
/// End-to-end tests for the dashboard contract, exercised over HTTP against the real pipeline.
/// </summary>
public class DashboardApiTests(ApiFactory factory) : IClassFixture<ApiFactory>
{
    private const string BaseUrl = "/api/v1";

    [Fact]
    public async Task Endpoints_reject_an_anonymous_caller()
    {
        var response = await factory.CreateClient().GetAsync($"{BaseUrl}/zones");
        Assert.Equal(HttpStatusCode.Unauthorized, response.StatusCode);
    }

    [Fact]
    public async Task The_profile_is_created_on_first_request()
    {
        // Cognito owns sign-up, so the first authenticated request is the only signal we get
        // that a user exists — there is no registration endpoint to call.
        var client = factory.CreateUserClient("user-profile-1", "new@example.com");

        var profile = await client.GetFromJsonAsync<ProfileResponse>($"{BaseUrl}/me");

        Assert.NotNull(profile);
        Assert.Equal("user-profile-1", profile.Id);
        Assert.Equal("new@example.com", profile.Email);
    }

    [Fact]
    public async Task The_profile_can_be_updated()
    {
        var client = factory.CreateUserClient("user-profile-2");
        await client.GetAsync($"{BaseUrl}/me");

        var response = await client.PatchAsJsonAsync($"{BaseUrl}/me", new UpdateProfileRequest
        {
            DisplayName = "Renamed",
            Company = "Hermes",
            Timezone = "Europe/Zurich",
            NotificationEmail = null,
            NotifyOnZoneErrors = false
        });

        response.EnsureSuccessStatusCode();
        var profile = await response.Content.ReadFromJsonAsync<ProfileResponse>();

        Assert.Equal("Renamed", profile!.DisplayName);
        Assert.False(profile.NotifyOnZoneErrors);
    }

    [Fact]
    public async Task An_unknown_timezone_is_rejected_with_a_bindable_field_error()
    {
        var client = factory.CreateUserClient("user-profile-3");

        var response = await client.PatchAsJsonAsync($"{BaseUrl}/me", new UpdateProfileRequest
        {
            DisplayName = "Someone",
            Timezone = "Mars/Olympus_Mons",
            NotifyOnZoneErrors = true
        });

        Assert.Equal(HttpStatusCode.UnprocessableEntity, response.StatusCode);

        var problem = await response.Content.ReadFromJsonAsync<ProblemResponse>();
        // The key has to match the request-body property name or the dashboard cannot bind it
        // to the offending input.
        Assert.Contains("timezone", problem!.Errors!.Keys);
    }

    [Fact]
    public async Task A_created_zone_is_pending_and_carries_its_nameservers()
    {
        var client = factory.CreateUserClient("user-zone-1");

        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones", new CreateZoneRequest { Apex = "Created.Example " });

        Assert.Equal(HttpStatusCode.Created, response.StatusCode);

        var zone = await response.Content.ReadFromJsonAsync<ZoneDetailResponse>();
        Assert.Equal("created.example", zone!.Apex);
        Assert.Equal("pending", zone.Status);
        Assert.NotEmpty(zone.Nameservers);
        Assert.NotNull(zone.Soa);

        // The Location header must actually resolve — the version token in the route is easy
        // to get wrong and only shows up here.
        Assert.NotNull(response.Headers.Location);
        var followed = await client.GetAsync(response.Headers.Location);
        followed.EnsureSuccessStatusCode();
    }

    [Fact]
    public async Task A_duplicate_apex_is_a_conflict()
    {
        var client = factory.CreateUserClient("user-zone-2");
        await client.PostAsJsonAsync($"{BaseUrl}/zones", new CreateZoneRequest { Apex = "dupe.example" });

        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones", new CreateZoneRequest { Apex = "dupe.example" });

        Assert.Equal(HttpStatusCode.Conflict, response.StatusCode);
    }

    [Fact]
    public async Task One_user_cannot_see_or_reach_anothers_zone()
    {
        var owner = factory.CreateUserClient("user-owner");
        var created = await owner.PostAsJsonAsync(
            $"{BaseUrl}/zones", new CreateZoneRequest { Apex = "private.example" });
        var zone = await created.Content.ReadFromJsonAsync<ZoneDetailResponse>();

        var stranger = factory.CreateUserClient("user-stranger");

        var list = await stranger.GetFromJsonAsync<List<ZoneResponse>>($"{BaseUrl}/zones");
        Assert.DoesNotContain(list!, z => z.Apex == "private.example");

        // 404, not 403: a 403 would confirm the id exists.
        var direct = await stranger.GetAsync($"{BaseUrl}/zones/{zone!.Id}");
        Assert.Equal(HttpStatusCode.NotFound, direct.StatusCode);
    }

    [Fact]
    public async Task An_apex_cname_is_stored_and_returned_as_alias()
    {
        // The flattening rule, end to end. The dashboard relies on getting ALIAS back so it
        // knows to render the "this CNAME is flattened" note.
        var client = factory.CreateUserClient("user-alias");
        var zone = await CreateZoneAsync(client, "flat.example");

        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records",
            new RecordRequest
            {
                Name = "@",
                Type = "CNAME",
                Ttl = 300,
                Data = "origin.hermes-edge.net"
            });

        Assert.Equal(HttpStatusCode.Created, response.StatusCode);

        var record = await response.Content.ReadFromJsonAsync<RecordResponse>();
        Assert.Equal("ALIAS", record!.Type);
        Assert.Equal("flat.example", record.Name);
    }

    [Fact]
    public async Task Alias_cannot_be_submitted_directly()
    {
        var client = factory.CreateUserClient("user-alias-direct");
        var zone = await CreateZoneAsync(client, "noalias.example");

        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records",
            new RecordRequest { Name = "@", Type = "ALIAS", Ttl = 300, Data = "target.example" });

        Assert.Equal(HttpStatusCode.UnprocessableEntity, response.StatusCode);
    }

    [Fact]
    public async Task An_unsupported_record_type_is_rejected()
    {
        // Hermes cannot build this type, and accepting it would take the whole zone offline on
        // the DNS side rather than just the one record.
        var client = factory.CreateUserClient("user-badtype");
        var zone = await CreateZoneAsync(client, "badtype.example");

        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records",
            new RecordRequest { Name = "@", Type = "DNSKEY", Ttl = 300, Data = "x" });

        Assert.Equal(HttpStatusCode.UnprocessableEntity, response.StatusCode);

        var problem = await response.Content.ReadFromJsonAsync<ProblemResponse>();
        Assert.Contains("type", problem!.Errors!.Keys);
    }

    [Fact]
    public async Task A_record_survives_create_update_and_delete()
    {
        var client = factory.CreateUserClient("user-crud");
        var zone = await CreateZoneAsync(client, "crud.example");

        var created = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records",
            new RecordRequest
            {
                Name = "www", Type = "A", Ttl = 300, Data = "192.0.2.1", Notes = "origin"
            });
        var record = await created.Content.ReadFromJsonAsync<RecordResponse>();

        Assert.Equal("www.crud.example", record!.Name);
        Assert.Equal("origin", record.Notes);

        var updated = await client.PutAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records/{record.Id}",
            new RecordRequest
            {
                Name = "www", Type = "A", Ttl = 60, Data = "192.0.2.2", Notes = "failover"
            });
        updated.EnsureSuccessStatusCode();

        var after = await updated.Content.ReadFromJsonAsync<RecordResponse>();
        Assert.Equal("192.0.2.2", after!.Data);
        Assert.Equal(60u, after.Ttl);

        var deleted = await client.DeleteAsync($"{BaseUrl}/zones/{zone.Id}/records/{record.Id}");
        Assert.Equal(HttpStatusCode.NoContent, deleted.StatusCode);

        var remaining = await client.GetFromJsonAsync<List<RecordResponse>>(
            $"{BaseUrl}/zones/{zone.Id}/records");
        Assert.Empty(remaining!);
    }

    [Fact]
    public async Task An_invalid_address_comes_back_as_a_field_error()
    {
        var client = factory.CreateUserClient("user-badip");
        var zone = await CreateZoneAsync(client, "badip.example");

        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records",
            new RecordRequest { Name = "www", Type = "A", Ttl = 300, Data = "not-an-ip" });

        Assert.Equal(HttpStatusCode.UnprocessableEntity, response.StatusCode);

        var problem = await response.Content.ReadFromJsonAsync<ProblemResponse>();
        Assert.Contains("data", problem!.Errors!.Keys);
    }

    [Fact]
    public async Task The_soa_serial_is_server_managed()
    {
        var client = factory.CreateUserClient("user-soa");
        var zone = await CreateZoneAsync(client, "soa.example");

        var response = await client.PutAsJsonAsync($"{BaseUrl}/zones/{zone.Id}/soa", new SoaModel
        {
            Mname = zone.Soa.Mname,
            Rname = "admin.soa.example",
            Serial = 1,
            Refresh = 3600,
            Retry = 600,
            Expire = 604800,
            Minimum = 60,
            Ttl = 3600
        });

        response.EnsureSuccessStatusCode();
        var updated = await response.Content.ReadFromJsonAsync<ZoneDetailResponse>();

        Assert.Equal("admin.soa.example", updated!.Soa.Rname);
        Assert.True(updated.Soa.Serial > 1);
    }

    [Fact]
    public async Task Deleting_a_zone_removes_its_records_too()
    {
        var client = factory.CreateUserClient("user-zonedelete");
        var zone = await CreateZoneAsync(client, "gone.example");
        await client.PostAsJsonAsync(
            $"{BaseUrl}/zones/{zone.Id}/records",
            new RecordRequest { Name = "www", Type = "A", Ttl = 300, Data = "192.0.2.1" });

        var deleted = await client.DeleteAsync($"{BaseUrl}/zones/{zone.Id}");
        Assert.Equal(HttpStatusCode.NoContent, deleted.StatusCode);

        var gone = await client.GetAsync($"{BaseUrl}/zones/{zone.Id}");
        Assert.Equal(HttpStatusCode.NotFound, gone.StatusCode);
        Assert.DoesNotContain(factory.Store.Records.Values, r => r.ZoneId == zone.Id);
    }

    [Fact]
    public async Task Responses_are_camel_cased()
    {
        // The dashboard's TypeScript reads these property names directly.
        var client = factory.CreateUserClient("user-casing");
        await CreateZoneAsync(client, "casing.example");

        var json = await client.GetStringAsync($"{BaseUrl}/zones");
        using var document = JsonDocument.Parse(json);
        var zone = document.RootElement.EnumerateArray().First();

        Assert.True(zone.TryGetProperty("recordCount", out _));
        Assert.True(zone.TryGetProperty("statusMessage", out _));
        Assert.False(zone.TryGetProperty("RecordCount", out _));

        // OwnerId is internal and must never reach a client.
        Assert.False(zone.TryGetProperty("ownerId", out _));
    }

    [Fact]
    public async Task Delegation_reports_not_delegated_when_nothing_is_observed()
    {
        var client = factory.CreateUserClient("user-delegation");
        var zone = await CreateZoneAsync(client, "delegation.example");

        var check = await client.GetFromJsonAsync<DelegationCheckResponse>(
            $"{BaseUrl}/zones/{zone.Id}/delegation");

        Assert.False(check!.Delegated);
        Assert.NotEmpty(check.ExpectedNameservers);
    }

    private static async Task<ZoneDetailResponse> CreateZoneAsync(HttpClient client, string apex)
    {
        var response = await client.PostAsJsonAsync(
            $"{BaseUrl}/zones", new CreateZoneRequest { Apex = apex });
        response.EnsureSuccessStatusCode();

        return (await response.Content.ReadFromJsonAsync<ZoneDetailResponse>())!;
    }
}
