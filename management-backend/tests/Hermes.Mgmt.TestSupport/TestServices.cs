using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service;
using Hermes.Mgmt.Service.Delegation;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.Extensions.Options;
using Microsoft.Extensions.Time.Testing;

namespace Hermes.Mgmt.TestSupport;

/// <summary>
/// Assembles the service layer over in-memory repositories with a controllable clock.
/// </summary>
/// <remarks>
/// Constructed by hand rather than through a DI container: a test that fails should point at
/// the code under test, not at a resolution error three layers deep.
/// </remarks>
public sealed class TestServices
{
    public TestServices(ZoneDefaultsOptions? defaults = null, DateTimeOffset? now = null)
    {
        Clock = new FakeTimeProvider(now ?? new DateTimeOffset(2026, 7, 21, 12, 0, 0, TimeSpan.Zero));
        Defaults = defaults ?? new ZoneDefaultsOptions
        {
            Nameservers = ["ns1.hermes-dns.net", "ns2.hermes-dns.net"]
        };

        Store = new InMemoryStore();
        Profiles = new InMemoryProfileRepository(Store);
        ZoneRepo = new InMemoryZoneRepository(Store);
        RecordRepo = new InMemoryRecordRepository(Store);
        Cache = new RecordingCacheProvider();
        Notifier = new RecordingNotifier();
        Delegation = new StubDelegationChecker();

        var publisher = new ZonePublisher(Cache, Notifier, NullLogger<ZonePublisher>.Instance);
        var options = Microsoft.Extensions.Options.Options.Create(Defaults);

        DelegationOptions = new DelegationOptions { Enabled = true, BatchSize = 50 };

        ProfileService = new ProfileService(Profiles, Clock);
        ZoneService = new ZoneService(ZoneRepo, Delegation, publisher, options, Clock);
        Scanner = new DelegationScanner(
            ZoneRepo,
            Delegation,
            publisher,
            Microsoft.Extensions.Options.Options.Create(DelegationOptions),
            Clock,
            NullLogger<DelegationScanner>.Instance);
        RecordService = new RecordService(ZoneRepo, RecordRepo, publisher, options, Clock);
        SyncService = new ZoneSyncService(
            ZoneRepo, RecordRepo, Cache,
            Microsoft.Extensions.Options.Options.Create(new CacheOptions()));
    }

    public FakeTimeProvider Clock { get; }
    public ZoneDefaultsOptions Defaults { get; }
    public InMemoryStore Store { get; }
    public InMemoryProfileRepository Profiles { get; }
    public InMemoryZoneRepository ZoneRepo { get; }
    public InMemoryRecordRepository RecordRepo { get; }
    public RecordingCacheProvider Cache { get; }
    public RecordingNotifier Notifier { get; }
    public StubDelegationChecker Delegation { get; }

    public DelegationOptions DelegationOptions { get; }

    public IProfileService ProfileService { get; }
    public IZoneService ZoneService { get; }
    public DelegationScanner Scanner { get; }
    public IRecordService RecordService { get; }
    public IZoneSyncService SyncService { get; }

    public Caller Caller { get; } = new("user-1", "user@example.com", "Test User");
    public Caller OtherCaller { get; } = new("user-2", "other@example.com", "Other User");

    /// <summary>Seeds a ready-to-use zone, bypassing the create path.</summary>
    public Zone SeedZone(
        string apex = "example.com",
        string? ownerId = null,
        ZoneStatus status = ZoneStatus.Active)
    {
        var now = Clock.GetUtcNow();
        var zone = new Zone
        {
            Id = $"zone_{apex.Replace('.', '_')}",
            Apex = apex,
            OwnerId = ownerId ?? Caller.UserId,
            Status = status,
            Version = 1,
            Nameservers = Defaults.Nameservers,
            RecordCount = 0,
            // Pending zones are due immediately, matching what CreateAsync sets; an active
            // zone carries no due time and so is absent from the scanner's index.
            NextDelegationCheckAt = status == ZoneStatus.Pending ? now : null,
            CreatedAt = now,
            UpdatedAt = now,
            Soa = new Soa
            {
                Mname = Defaults.Nameservers[0],
                Rname = $"hostmaster.{apex}",
                Serial = new Serial(2026072101),
                Refresh = 3600,
                Retry = 600,
                Expire = 604800,
                Minimum = 60,
                Ttl = 3600
            }
        };

        Store.Seed(zone);
        return zone;
    }
}
