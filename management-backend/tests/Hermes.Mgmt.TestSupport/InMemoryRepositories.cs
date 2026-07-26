using System.Collections.Concurrent;
using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service;

namespace Hermes.Mgmt.TestSupport;

/// <summary>
/// In-memory stand-ins for the repositories.
/// </summary>
/// <remarks>
/// Hand-written rather than mock-framework doubles because these have to enforce the same
/// invariants the real DynamoDB implementation does — conditional creates, atomic version
/// bumps, cascading deletes. A mock that returns whatever a test tells it to would let a
/// service pass despite relying on behaviour DynamoDB does not actually provide.
/// </remarks>
public sealed class InMemoryStore
{
    public ConcurrentDictionary<string, UserProfile> Profiles { get; } = new();
    public ConcurrentDictionary<string, Zone> Zones { get; } = new();

    /// <summary>Records keyed by <c>{zoneId}/{recordId}</c>.</summary>
    public ConcurrentDictionary<string, DnsRecord> Records { get; } = new();

    /// <summary>Apex to zone id, mirroring the real table's uniqueness claim.</summary>
    public ConcurrentDictionary<string, string> ApexClaims { get; } = new();

    public void Seed(Zone zone)
    {
        Zones[zone.Id] = zone;
        ApexClaims[zone.Apex] = zone.Id;
    }

    public void Seed(DnsRecord record) => Records[$"{record.ZoneId}/{record.Id}"] = record;
}

public sealed class InMemoryProfileRepository(InMemoryStore store) : IProfileRepository
{
    public Task<UserProfile?> GetAsync(string userId, CancellationToken ct = default) =>
        Task.FromResult(store.Profiles.GetValueOrDefault(userId));

    public Task PutAsync(UserProfile profile, CancellationToken ct = default)
    {
        store.Profiles[profile.Id] = profile;
        return Task.CompletedTask;
    }
}

public sealed class InMemoryZoneRepository(InMemoryStore store) : IZoneRepository
{
    public Task<Zone?> GetAsync(string zoneId, CancellationToken ct = default) =>
        Task.FromResult(store.Zones.GetValueOrDefault(zoneId));

    public Task<IReadOnlyList<Zone>> ListByOwnerAsync(string ownerId, CancellationToken ct = default) =>
        Task.FromResult<IReadOnlyList<Zone>>(store.Zones.Values
            .Where(z => z.OwnerId == ownerId)
            .OrderBy(z => z.Apex, StringComparer.Ordinal)
            .ToList());

    public Task<IReadOnlyList<ZoneRef>> ListAllRefsAsync(CancellationToken ct = default) =>
        Task.FromResult<IReadOnlyList<ZoneRef>>(store.Zones.Values
            .Select(z => new ZoneRef(z.Id, z.Apex, z.Version))
            .ToList());

    public Task<bool> TryCreateAsync(Zone zone, CancellationToken ct = default)
    {
        // TryAdd, matching the real conditional write: the second caller for an apex loses.
        if (!store.ApexClaims.TryAdd(zone.Apex, zone.Id))
        {
            return Task.FromResult(false);
        }

        store.Zones[zone.Id] = zone;
        return Task.FromResult(true);
    }

    public Task<Zone> UpdateAsync(Zone zone, CancellationToken ct = default)
    {
        var updated = zone with { Version = zone.Version + 1 };
        store.Zones[zone.Id] = updated;
        return Task.FromResult(updated);
    }

    public Task<ulong?> BumpVersionAsync(
        string zoneId, int recordCountDelta, DateTimeOffset now, CancellationToken ct = default)
    {
        if (!store.Zones.TryGetValue(zoneId, out var zone))
        {
            return Task.FromResult<ulong?>(null);
        }

        var updated = zone with
        {
            Version = zone.Version + 1,
            Soa = zone.Soa with { Serial = new Serial(zone.Soa.Serial.Value + 1) },
            RecordCount = Math.Max(0, zone.RecordCount + recordCountDelta),
            UpdatedAt = now
        };

        store.Zones[zoneId] = updated;
        return Task.FromResult<ulong?>(updated.Version);
    }

    /// <summary>
    /// Runs after a due-zone query returns, before the caller claims anything.
    /// </summary>
    /// <remarks>
    /// The seam for simulating another replica winning the claim: that race only exists in the
    /// window between the query and the conditional write, so a test has to be able to act
    /// inside it.
    /// </remarks>
    public Action? AfterListDueForDelegationCheck { get; set; }

    public Task<IReadOnlyList<Zone>> ListDueForDelegationCheckAsync(
        DateTimeOffset now, int limit, CancellationToken ct = default)
    {
        // Mirrors the sparse index: only pending zones with a due time are in it at all.
        var due = store.Zones.Values
            .Where(z => z.Status == ZoneStatus.Pending
                        && z.NextDelegationCheckAt is { } at
                        && at <= now)
            .OrderBy(z => z.NextDelegationCheckAt)
            .Take(limit)
            .ToList();

        AfterListDueForDelegationCheck?.Invoke();

        return Task.FromResult<IReadOnlyList<Zone>>(due);
    }

    public Task<bool> TryClaimDelegationCheckAsync(
        string zoneId,
        DateTimeOffset? expectedDueAt,
        DateTimeOffset nextDueAt,
        CancellationToken ct = default)
    {
        if (!store.Zones.TryGetValue(zoneId, out var zone)
            || zone.Status != ZoneStatus.Pending
            || zone.NextDelegationCheckAt != expectedDueAt)
        {
            return Task.FromResult(false);
        }

        // Compare-and-set, like the real conditional write: whoever gets here first with the
        // value they read wins, and a loser must not proceed to the DNS lookup.
        var claimed = zone with { NextDelegationCheckAt = nextDueAt };
        var won = store.Zones.TryUpdate(zoneId, claimed, zone);

        return Task.FromResult(won);
    }

    public Task DeleteAsync(string zoneId, string apex, CancellationToken ct = default)
    {
        store.Zones.TryRemove(zoneId, out _);
        store.ApexClaims.TryRemove(apex, out _);

        foreach (var key in store.Records.Keys.Where(k => k.StartsWith($"{zoneId}/", StringComparison.Ordinal)))
        {
            store.Records.TryRemove(key, out _);
        }

        return Task.CompletedTask;
    }
}

public sealed class InMemoryRecordRepository(InMemoryStore store) : IRecordRepository
{
    public Task<IReadOnlyList<DnsRecord>> ListByZoneAsync(string zoneId, CancellationToken ct = default) =>
        Task.FromResult<IReadOnlyList<DnsRecord>>(store.Records.Values
            .Where(r => r.ZoneId == zoneId)
            .ToList());

    public Task<DnsRecord?> GetAsync(string zoneId, string recordId, CancellationToken ct = default) =>
        Task.FromResult(store.Records.GetValueOrDefault($"{zoneId}/{recordId}"));

    public Task PutAsync(DnsRecord record, CancellationToken ct = default)
    {
        store.Records[$"{record.ZoneId}/{record.Id}"] = record;
        return Task.CompletedTask;
    }

    public Task<bool> DeleteAsync(string zoneId, string recordId, CancellationToken ct = default) =>
        Task.FromResult(store.Records.TryRemove($"{zoneId}/{recordId}", out _));
}

/// <summary>
/// Records what was invalidated, so tests can assert that writes publish.
/// </summary>
public sealed class RecordingCacheProvider : ICacheProvider
{
    private readonly Dictionary<string, object> _entries = [];

    public List<string> Removed { get; } = [];

    /// <summary>When false, every read misses — the default, matching a disabled cache.</summary>
    public bool ServeReads { get; set; }

    public Task<T?> GetAsync<T>(string key, CancellationToken ct = default) where T : class =>
        Task.FromResult(ServeReads && _entries.TryGetValue(key, out var value) ? value as T : null);

    public Task SetAsync<T>(string key, T value, TimeSpan ttl, CancellationToken ct = default)
        where T : class
    {
        _entries[key] = value;
        return Task.CompletedTask;
    }

    public Task RemoveAsync(string key, CancellationToken ct = default)
    {
        Removed.Add(key);
        _entries.Remove(key);
        return Task.CompletedTask;
    }
}

/// <summary>Counts reload notifications.</summary>
public sealed class RecordingNotifier : IHermesNotifier
{
    public int Calls { get; private set; }

    public Task NotifyAsync(CancellationToken ct = default)
    {
        Calls++;
        return Task.CompletedTask;
    }
}

/// <summary>Returns a scripted set of nameservers.</summary>
public sealed class StubDelegationChecker : IDelegationChecker
{
    public List<string> Nameservers { get; set; } = [];

    /// <summary>Set to simulate a resolver failure.</summary>
    public Exception? Throws { get; set; }

    public Task<IReadOnlyList<string>> LookupNameserversAsync(string apex, CancellationToken ct = default) =>
        Throws is not null
            ? Task.FromException<IReadOnlyList<string>>(Throws)
            : Task.FromResult<IReadOnlyList<string>>(Nameservers);
}
