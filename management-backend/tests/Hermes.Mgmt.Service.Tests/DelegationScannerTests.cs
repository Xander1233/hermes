using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service.Delegation;
using Hermes.Mgmt.Service.Validation;
using Hermes.Mgmt.TestSupport;

namespace Hermes.Mgmt.Service.Tests;

public class DelegationPolicyTests
{
    private static readonly string[] Expected = ["ns1.hermes-dns.net", "ns2.hermes-dns.net"];

    [Fact]
    public void All_expected_nameservers_present_counts_as_delegated() =>
        Assert.True(DelegationPolicy.IsDelegated(Expected, [.. Expected]));

    [Fact]
    public void Matching_is_case_insensitive() =>
        Assert.True(DelegationPolicy.IsDelegated(
            Expected, ["NS1.Hermes-DNS.net", "NS2.HERMES-DNS.NET"]));

    [Fact]
    public void A_partial_match_is_not_delegated()
    {
        // A registrar mid-edit often shows a mix of old and new. Activating on a partial match
        // would mark a domain live while it still resolves through the old provider half the
        // time.
        Assert.False(DelegationPolicy.IsDelegated(Expected, ["ns1.hermes-dns.net"]));
    }

    [Fact]
    public void Extra_nameservers_alongside_ours_are_tolerated()
    {
        // Some registrars add their own; the delegation still works.
        Assert.True(DelegationPolicy.IsDelegated(
            Expected, [.. Expected, "ns3.registrar.example"]));
    }

    [Fact]
    public void Someone_elses_nameservers_are_not_delegated() =>
        Assert.False(DelegationPolicy.IsDelegated(
            Expected, ["ns1.otherdns.com", "ns2.otherdns.com"]));

    [Fact]
    public void Observing_nothing_is_not_delegated() =>
        Assert.False(DelegationPolicy.IsDelegated(Expected, []));

    [Theory]
    [InlineData(0, 2)]        // minutes old  -> every 2 minutes
    [InlineData(5, 15)]       // hours old    -> every 15 minutes
    [InlineData(48, 60)]      // 2 days old   -> hourly
    [InlineData(24 * 30, 360)] // a month old -> every 6 hours
    public void The_check_interval_backs_off_with_age(int ageHours, int expectedMinutes)
    {
        var now = new DateTimeOffset(2026, 7, 21, 12, 0, 0, TimeSpan.Zero);
        var createdAt = now.AddHours(-ageHours);

        var next = DelegationPolicy.NextCheckAt(createdAt, now);

        Assert.Equal(TimeSpan.FromMinutes(expectedMinutes), next - now);
    }
}

public class DelegationScannerTests
{
    private static void Delegate(TestServices sut) =>
        sut.Delegation.Nameservers = [.. sut.Defaults.Nameservers];

    [Fact]
    public async Task A_delegated_pending_zone_is_activated()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        Delegate(sut);

        var result = await sut.Scanner.RunOnceAsync();

        Assert.Equal(1, result.Activated);
        Assert.Equal(ZoneStatus.Active, sut.Store.Zones[zone.Id].Status);
    }

    [Fact]
    public async Task Activation_notifies_the_dns_fleet()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        Delegate(sut);
        sut.Cache.Removed.Clear();

        await sut.Scanner.RunOnceAsync();

        Assert.Contains(CacheKeys.ZoneDocument(zone.Id), sut.Cache.Removed);
        Assert.True(sut.Notifier.Calls > 0);
    }

    [Fact]
    public async Task An_activated_zone_leaves_the_scanner_index()
    {
        // Clearing the due time is what removes it from the sparse index — there is no
        // separate cleanup step, so this is the thing that must hold.
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        Delegate(sut);

        await sut.Scanner.RunOnceAsync();

        Assert.Null(sut.Store.Zones[zone.Id].NextDelegationCheckAt);
        Assert.Empty(await sut.ZoneRepo.ListDueForDelegationCheckAsync(
            sut.Clock.GetUtcNow().AddYears(1), 50));
    }

    [Fact]
    public async Task An_undelegated_zone_stays_pending_and_is_rescheduled()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        sut.Delegation.Nameservers = ["ns1.someoneelse.com"];

        var result = await sut.Scanner.RunOnceAsync();

        Assert.Equal(0, result.Activated);
        Assert.Equal(1, result.StillPending);

        var stored = sut.Store.Zones[zone.Id];
        Assert.Equal(ZoneStatus.Pending, stored.Status);
        Assert.True(stored.NextDelegationCheckAt > sut.Clock.GetUtcNow());
    }

    [Fact]
    public async Task A_zone_not_yet_due_is_left_alone()
    {
        var sut = new TestServices();
        sut.SeedZone(status: ZoneStatus.Pending);
        Delegate(sut);

        await sut.Scanner.RunOnceAsync();          // activates it
        var second = await sut.Scanner.RunOnceAsync();

        Assert.Equal(0, second.Examined);
    }

    [Fact]
    public async Task Active_zones_are_never_examined()
    {
        var sut = new TestServices();
        sut.SeedZone("active.example");
        Delegate(sut);

        var result = await sut.Scanner.RunOnceAsync();

        Assert.Equal(0, result.Examined);
    }

    [Fact]
    public async Task One_failing_lookup_does_not_stop_the_pass()
    {
        var sut = new TestServices();
        sut.SeedZone("first.example", status: ZoneStatus.Pending);
        sut.SeedZone("second.example", status: ZoneStatus.Pending);
        sut.Delegation.Throws = new InvalidOperationException("resolver exploded");

        var result = await sut.Scanner.RunOnceAsync();

        // Both were examined and both survived; the exception was contained per zone.
        Assert.Equal(2, result.Examined);
        Assert.Equal(2, result.StillPending);
    }

    [Fact]
    public async Task A_zone_another_replica_already_claimed_is_skipped()
    {
        // The lease that makes this safe on several replicas: losing the race must mean not
        // doing the lookup, not doing it twice.
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        Delegate(sut);

        // The race exists only between the query and the claim, so the other replica has to
        // win inside that window — claiming beforehand would just take the zone off the due
        // list and prove nothing.
        sut.ZoneRepo.AfterListDueForDelegationCheck = () =>
        {
            var current = sut.Store.Zones[zone.Id];
            sut.Store.Zones[zone.Id] = current with
            {
                NextDelegationCheckAt = sut.Clock.GetUtcNow().AddMinutes(2)
            };
        };

        var result = await sut.Scanner.RunOnceAsync();

        Assert.Equal(1, result.Skipped);
        Assert.Equal(0, result.Activated);
        Assert.Equal(ZoneStatus.Pending, sut.Store.Zones[zone.Id].Status);
    }

    [Fact]
    public async Task Claiming_twice_with_the_same_expected_value_fails_the_second_time()
    {
        var sut = new TestServices();
        var zone = sut.SeedZone(status: ZoneStatus.Pending);
        var due = zone.NextDelegationCheckAt;
        var next = sut.Clock.GetUtcNow().AddMinutes(2);

        Assert.True(await sut.ZoneRepo.TryClaimDelegationCheckAsync(zone.Id, due, next));
        Assert.False(await sut.ZoneRepo.TryClaimDelegationCheckAsync(zone.Id, due, next));
    }

    [Fact]
    public async Task A_pass_is_bounded_by_the_batch_size()
    {
        var sut = new TestServices();
        sut.DelegationOptions.BatchSize = 2;
        for (var i = 0; i < 5; i++)
        {
            sut.SeedZone($"zone{i}.example", status: ZoneStatus.Pending);
        }

        var result = await sut.Scanner.RunOnceAsync();

        Assert.Equal(2, result.Examined);
    }

    [Fact]
    public async Task The_scanner_does_nothing_when_delegation_checking_is_disabled()
    {
        var sut = new TestServices();
        sut.DelegationOptions.Enabled = false;
        sut.SeedZone(status: ZoneStatus.Pending);
        Delegate(sut);

        Assert.Equal(ScanResult.Empty, await sut.Scanner.RunOnceAsync());
    }
}

/// <summary>
/// The guarantee that a zone is usable — and served — before it is delegated.
/// </summary>
/// <remarks>
/// This is the whole reason zones are published from creation rather than from activation: the
/// records a user adds while waiting on their registrar have to already be at the DNS servers
/// when the delegation lands. If publication waited for activation, every zone would pass
/// through a window where the parent points at Hermes but Hermes has no zone, and every query
/// in that window would fail.
/// </remarks>
public class PendingZoneUsabilityTests
{
    private static RecordDraft Draft(string name, string data) =>
        new() { Name = name, Type = RecordType.A, Ttl = 300, Data = data };

    [Fact]
    public async Task Records_can_be_added_to_a_zone_that_is_not_delegated_yet()
    {
        var sut = new TestServices();
        var zone = await sut.ZoneService.CreateAsync(sut.Caller.UserId, "pending.example");
        Assert.Equal(ZoneStatus.Pending, zone.Status);

        var record = await sut.RecordService.CreateAsync(
            sut.Caller.UserId, zone.Id, Draft("www", "192.0.2.1"));

        Assert.Equal("www.pending.example", record.Name);
    }

    [Fact]
    public async Task A_pending_zone_is_already_in_the_manifest_hermes_pulls()
    {
        var sut = new TestServices();
        var zone = await sut.ZoneService.CreateAsync(sut.Caller.UserId, "pending.example");

        var manifest = await sut.SyncService.GetManifestAsync();

        Assert.Contains(manifest.Zones, z => z.Id == zone.Id);
    }

    [Fact]
    public async Task A_pending_zones_records_are_already_served_to_hermes()
    {
        var sut = new TestServices();
        var zone = await sut.ZoneService.CreateAsync(sut.Caller.UserId, "pending.example");
        await sut.RecordService.CreateAsync(sut.Caller.UserId, zone.Id, Draft("www", "192.0.2.1"));

        var document = await sut.SyncService.GetZoneDocumentAsync(zone.Id);

        Assert.Equal("www.pending.example", Assert.Single(document!.Records).Name);
    }

    [Fact]
    public async Task Activation_does_not_change_what_hermes_is_served()
    {
        // Activation is a status flag for the dashboard, not a publication gate. The zone
        // content Hermes sees must be identical either side of it.
        var sut = new TestServices();
        var zone = await sut.ZoneService.CreateAsync(sut.Caller.UserId, "pending.example");
        await sut.RecordService.CreateAsync(sut.Caller.UserId, zone.Id, Draft("www", "192.0.2.1"));

        var before = await sut.SyncService.GetZoneDocumentAsync(zone.Id);

        sut.Delegation.Nameservers = [.. sut.Defaults.Nameservers];
        Assert.Equal(1, (await sut.Scanner.RunOnceAsync()).Activated);

        var after = await sut.SyncService.GetZoneDocumentAsync(zone.Id);

        Assert.Equal(before!.Records.Count, after!.Records.Count);
        Assert.Equal(before.Records[0].Name, after.Records[0].Name);
        Assert.Equal(before.Records[0].Data, after.Records[0].Data);
        Assert.Equal(before.Soa, after.Soa);
    }
}
