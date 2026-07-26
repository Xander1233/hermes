using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Common.Errors;
using Hermes.Mgmt.Service.Validation;
using Hermes.Mgmt.TestSupport;

namespace Hermes.Mgmt.Service.Tests;

public class RecordServiceTests
{
    private static RecordDraft Draft(
        string name, RecordType type, string? data, ushort? preference = null, string? notes = null) =>
        new()
        {
            Name = name,
            Type = type,
            Ttl = 300,
            Data = data,
            Preference = preference,
            Notes = notes
        };

    [Fact]
    public async Task Create_expands_a_relative_name_to_an_fqdn()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.A, "192.0.2.1"));

        Assert.Equal("www.example.com", record.Name);
    }

    [Fact]
    public async Task An_apex_cname_is_stored_as_an_alias()
    {
        // The flattening rule: a CNAME cannot coexist with the apex's SOA and NS records, so
        // Hermes serves an ALIAS instead. The dashboard renders it back as "CNAME".
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("@", RecordType.CNAME, "origin.hermes-edge.net"));

        Assert.Equal(RecordType.ALIAS, record.Type);
        Assert.Equal("example.com", record.Name);
    }

    [Fact]
    public async Task A_cname_below_the_apex_stays_a_cname()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.CNAME, "example.com"));

        Assert.Equal(RecordType.CNAME, record.Type);
    }

    [Fact]
    public async Task Fields_that_do_not_belong_to_the_type_are_cleared()
    {
        // They would otherwise be serialized into the zone-sync payload and travel with the
        // record forever.
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id,
            Draft("www", RecordType.A, "192.0.2.1", preference: 10));

        Assert.Null(record.Preference);
    }

    [Fact]
    public async Task Mx_keeps_its_preference()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id,
            Draft("@", RecordType.MX, "mail.example.com", preference: 10));

        Assert.Equal((ushort)10, record.Preference);
    }

    [Fact]
    public async Task A_name_from_another_domain_is_treated_as_relative_and_appended()
    {
        // The BIND convention, which the dashboard mirrors: a name without a trailing dot is
        // relative to the origin. So this cannot land outside the zone — it becomes a
        // subdomain of it. Surprising enough to pin, and the reason the record dialog shows
        // the resulting FQDN under the name field.
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www.other.com", RecordType.A, "192.0.2.1"));

        Assert.Equal("www.other.com.example.com", record.Name);
    }

    [Fact]
    public async Task An_over_long_name_is_rejected()
    {
        // The in-zone check in DnsValidator is unreachable through ToFqdn, but syntax errors
        // are not: a label the expansion cannot fix still has to be caught.
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.RecordService.CreateAsync(
                sut.Caller.UserId, zone.Id,
                Draft(new string('a', 64), RecordType.A, "192.0.2.1")));

        Assert.Contains("name", ex.Errors);
    }

    [Fact]
    public async Task Create_rejects_a_cname_alongside_other_data()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();
        await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.A, "192.0.2.1"));

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.RecordService.CreateAsync(
                sut.Caller.UserId, zone.Id, Draft("www", RecordType.CNAME, "example.com")));

        Assert.Equal(System.Net.HttpStatusCode.Conflict, ex.Status);
    }

    [Fact]
    public async Task Update_does_not_conflict_with_the_record_being_updated()
    {
        // The candidate must be compared against its siblings only — comparing it against
        // itself would make every no-op save fail as a duplicate.
        var sut = new TestServices();
        var zone = sut.SeedZone();
        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.A, "192.0.2.1"));

        var updated = await sut.RecordService.UpdateAsync(
            sut.Caller.UserId, zone.Id, record.Id,
            Draft("www", RecordType.A, "192.0.2.1", notes: "same value, new note"));

        Assert.Equal("same value, new note", updated.Notes);
    }

    [Fact]
    public async Task Update_preserves_the_original_creation_timestamp()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();
        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.A, "192.0.2.1"));

        sut.Clock.Advance(TimeSpan.FromHours(1));

        var updated = await sut.RecordService.UpdateAsync(
            sut.Caller.UserId, zone.Id, record.Id, Draft("www", RecordType.A, "192.0.2.2"));

        Assert.Equal(record.CreatedAt, updated.CreatedAt);
        Assert.True(updated.UpdatedAt > record.UpdatedAt);
    }

    [Fact]
    public async Task Create_bumps_the_zone_version_and_record_count()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.A, "192.0.2.1"));

        var stored = sut.Store.Zones[zone.Id];
        Assert.Equal(zone.Version + 1, stored.Version);
        Assert.Equal(1, stored.RecordCount);
    }

    [Fact]
    public async Task Delete_decrements_the_record_count()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();
        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", RecordType.A, "192.0.2.1"));

        await sut.RecordService.DeleteAsync(sut.Caller.UserId, zone.Id, record.Id);

        Assert.Equal(0, sut.Store.Zones[zone.Id].RecordCount);
    }

    [Fact]
    public async Task Deleting_a_missing_record_is_not_found()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.RecordService.DeleteAsync(sut.Caller.UserId, zone.Id, "rec_nope"));

        Assert.Equal(System.Net.HttpStatusCode.NotFound, ex.Status);
    }

    [Fact]
    public async Task Records_in_another_users_zone_are_not_reachable()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(ownerId: sut.OtherCaller.UserId);

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.RecordService.ListAsync(sut.Caller.UserId, zone.Id));

        Assert.Equal(System.Net.HttpStatusCode.NotFound, ex.Status);
    }
}
