using Hermes.Mgmt.Common.Domain;

namespace Hermes.Mgmt.Common.Abstractions;

public interface IProfileRepository
{
    Task<UserProfile?> GetAsync(string userId, CancellationToken ct = default);

    Task PutAsync(UserProfile profile, CancellationToken ct = default);
}

public interface IZoneRepository
{
    Task<Zone?> GetAsync(string zoneId, CancellationToken ct = default);

    Task<IReadOnlyList<Zone>> ListByOwnerAsync(string ownerId, CancellationToken ct = default);

    Task<IReadOnlyList<ZoneRef>> ListAllRefsAsync(CancellationToken ct = default);

    Task<bool> TryCreateAsync(Zone zone, CancellationToken ct = default);

    Task<Zone> UpdateAsync(Zone zone, CancellationToken ct = default);

    Task<ulong?> BumpVersionAsync(string zoneId, int recordCountDelta, DateTimeOffset now, CancellationToken ct = default);

    Task<IReadOnlyList<Zone>> ListDueForDelegationCheckAsync(DateTimeOffset now, int limit, CancellationToken ct = default);

    Task<bool> TryClaimDelegationCheckAsync(string zoneId, DateTimeOffset? expectedDueAt, DateTimeOffset nextDueAt, CancellationToken ct = default);

    Task DeleteAsync(string zoneId, string apex, CancellationToken ct = default);
}

public interface IRecordRepository
{
    Task<IReadOnlyList<DnsRecord>> ListByZoneAsync(string zoneId, CancellationToken ct = default);

    Task<DnsRecord?> GetAsync(string zoneId, string recordId, CancellationToken ct = default);

    Task PutAsync(DnsRecord record, CancellationToken ct = default);

    Task<bool> DeleteAsync(string zoneId, string recordId, CancellationToken ct = default);
}
