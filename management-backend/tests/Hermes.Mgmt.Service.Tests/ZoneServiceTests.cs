using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Common.Errors;
using Hermes.Mgmt.Service;
using Hermes.Mgmt.Service.Options;
using Hermes.Mgmt.TestSupport;

namespace Hermes.Mgmt.Service.Tests;

public class ZoneServiceTests
{
    [Fact]
    public async Task Create_normalizes_the_apex_and_starts_pending()
    {
        var sut = new TestServices();

        var zone = await sut.ZoneService.CreateAsync(sut.Caller.UserId, "  Example.COM. ");

        Assert.Equal("example.com", zone.Apex);
        Assert.Equal(ZoneStatus.Pending, zone.Status);
        Assert.Equal(sut.Defaults.Nameservers, zone.Nameservers);
        Assert.Empty(await sut.RecordRepo.ListByZoneAsync(zone.Id));
    }

    [Fact]
    public async Task Create_rejects_a_name_that_is_not_a_domain()
    {
        var sut = new TestServices();

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.ZoneService.CreateAsync(sut.Caller.UserId, "localhost"));

        Assert.Contains("apex", ex.Errors);
    }

    [Fact]
    public async Task Create_rejects_an_apex_someone_else_already_owns()
    {
        var sut = new TestServices();
        await sut.ZoneService.CreateAsync(sut.OtherCaller.UserId, "example.com");

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.ZoneService.CreateAsync(sut.Caller.UserId, "example.com"));

        Assert.Equal(System.Net.HttpStatusCode.Conflict, ex.Status);
    }

    [Fact]
    public async Task Create_enforces_the_per_user_zone_limit()
    {
        var sut = new TestServices(new ZoneDefaultsOptions
        {
            Nameservers = ["ns1.hermes-dns.net"],
            MaxZonesPerUser = 1
        });

        await sut.ZoneService.CreateAsync(sut.Caller.UserId, "first.com");

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.ZoneService.CreateAsync(sut.Caller.UserId, "second.com"));

        Assert.Contains("apex", ex.Errors);
    }

    [Fact]
    public async Task List_only_returns_the_callers_zones()
    {
        var sut = new TestServices();
        sut.SeedZone("mine.com");
        sut.SeedZone("theirs.com", ownerId: sut.OtherCaller.UserId);

        var zones = await sut.ZoneService.ListAsync(sut.Caller.UserId);

        Assert.Equal(["mine.com"], zones.Select(z => z.Apex));
    }

    [Fact]
    public async Task Another_users_zone_reads_as_not_found()
    {
        // 404 rather than 403 on purpose: a 403 would confirm the id exists and let anyone
        // enumerate other tenants' zone ids.
        var sut = new TestServices();
        var zone = sut.SeedZone("theirs.com", ownerId: sut.OtherCaller.UserId);

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.ZoneService.GetAsync(sut.Caller.UserId, zone.Id));

        Assert.Equal(System.Net.HttpStatusCode.NotFound, ex.Status);
    }

    [Fact]
    public async Task Delete_removes_the_zone_its_records_and_its_apex_claim()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();
        sut.Store.Seed(new DnsRecord
        {
            Id = "r1", ZoneId = zone.Id, Name = "www.example.com",
            Type = RecordType.A, Ttl = 300, Data = "192.0.2.1"
        });

        await sut.ZoneService.DeleteAsync(sut.Caller.UserId, zone.Id);

        Assert.Empty(sut.Store.Zones);
        Assert.Empty(sut.Store.Records);
        // The claim must go too, or the user could never re-add the domain.
        Assert.Empty(sut.Store.ApexClaims);
    }

    [Fact]
    public async Task UpdateSoa_ignores_the_client_supplied_serial()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var updated = await sut.ZoneService.UpdateSoaAsync(sut.Caller.UserId, zone.Id, zone.Soa with
        {
            Rname = "admin.example.com",
            // A client that could set this could lower it, and a secondary would then refuse
            // every subsequent transfer.
            Serial = new Serial(1)
        });

        Assert.Equal("admin.example.com", updated.Soa.Rname);
        Assert.True(updated.Soa.Serial.Value > zone.Soa.Serial.Value);
    }

    [Fact]
    public async Task UpdateSoa_rejects_an_expire_below_refresh()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        var ex = await Assert.ThrowsAsync<AppException>(
            () => sut.ZoneService.UpdateSoaAsync(
                sut.Caller.UserId, zone.Id, zone.Soa with { Expire = 60 }));

        Assert.Contains("expire", ex.Errors);
    }

    [Fact]
    public async Task Delegation_flips_a_pending_zone_to_active()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        sut.Delegation.Nameservers = [.. sut.Defaults.Nameservers];

        var check = await sut.ZoneService.CheckDelegationAsync(sut.Caller.UserId, zone.Id);

        Assert.True(check.Delegated);
        Assert.Equal(ZoneStatus.Active, sut.Store.Zones[zone.Id].Status);
    }

    [Fact]
    public async Task Partial_delegation_does_not_count()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        sut.Delegation.Nameservers = [sut.Defaults.Nameservers[0]];

        var check = await sut.ZoneService.CheckDelegationAsync(sut.Caller.UserId, zone.Id);

        Assert.False(check.Delegated);
        Assert.Equal(ZoneStatus.Pending, sut.Store.Zones[zone.Id].Status);
    }

    [Fact]
    public async Task A_resolver_failure_reads_as_not_delegated_rather_than_erroring()
    {
        // The user is clicking this while editing their registrar; a resolver hiccup must not
        // surface as a 500 on their dashboard.
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        sut.Delegation.Throws = new InvalidOperationException("resolver exploded");

        var check = await sut.ZoneService.CheckDelegationAsync(sut.Caller.UserId, zone.Id);

        Assert.False(check.Delegated);
        Assert.Empty(check.ObservedNameservers);
    }

    [Fact]
    public async Task Writes_invalidate_the_sync_caches_and_notify_the_fleet()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone();

        await sut.ZoneService.UpdateSoaAsync(sut.Caller.UserId, zone.Id, zone.Soa);

        Assert.Contains(CacheKeys.Manifest, sut.Cache.Removed);
        Assert.Contains(CacheKeys.ZoneDocument(zone.Id), sut.Cache.Removed);
        Assert.Equal(1, sut.Notifier.Calls);
    }

    private static readonly DateTimeOffset SerialNow = new(2026, 7, 21, 12, 0, 0, TimeSpan.Zero);

    private static Serial Next(uint current) => new Serial(current).Next(SerialNow);

    [Fact]
    public void Serial_Next_uses_the_timestamp_when_it_is_ahead()
    {
        var unix = (uint)SerialNow.ToUnixTimeSeconds();

        // A serial behind wall-clock time jumps straight to the current timestamp.
        Assert.Equal(new Serial(unix), Next(1u));
        Assert.Equal(new Serial(unix), Next(unix - 1));
        Assert.Equal(new Serial(unix), Serial.FromTimestamp(SerialNow));
    }

    [Fact]
    public void Serial_Next_increments_when_the_timestamp_is_not_ahead()
    {
        var unix = (uint)SerialNow.ToUnixTimeSeconds();

        // A second change within the same second: the timestamp cannot advance, so clamp by one.
        Assert.Equal(new Serial(unix + 1), Next(unix));

        // A zone migrated from the old YYYYMMDDnn scheme sits above today's timestamp; it must
        // climb by one rather than jump backwards to the (smaller) timestamp.
        Assert.Equal(new Serial(2026072102u), Next(2026072101u));
    }

    [Fact]
    public void Serial_Next_survives_a_backward_clock_step()
    {
        var current = Next(1u); // == the timestamp
        // NTP steps the clock back a minute; the serial must still advance.
        Assert.Equal(new Serial(current.Value + 1), current.Next(SerialNow.AddMinutes(-1)));
    }

    [Theory]
    // A zone imported with a silly-large serial keeps climbing; it must never go backwards, or
    // secondaries would refuse the transfer.
    [InlineData(4000000000u, 4000000001u)]
    // Frozen at the 32-bit ceiling rather than wrapping to zero.
    [InlineData(uint.MaxValue, uint.MaxValue)]
    public void Serial_Next_never_decreases(uint current, uint expected) =>
        Assert.Equal(new Serial(expected), Next(current));
}
