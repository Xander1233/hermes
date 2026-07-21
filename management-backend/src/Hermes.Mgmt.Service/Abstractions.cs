using Hermes.Mgmt.Common.Contracts.Sync.V1;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service.Validation;

namespace Hermes.Mgmt.Service;

public sealed record Caller(string UserId, string Email, string? DisplayName);

public sealed record UpdateProfileCommand(
    string DisplayName,
    string? Company,
    string Timezone,
    string? NotificationEmail,
    bool NotifyOnZoneErrors);

public interface IProfileService
{
    Task<UserProfile> GetOrCreateAsync(Caller caller, CancellationToken ct = default);

    Task<UserProfile> UpdateAsync(Caller caller, UpdateProfileCommand command, CancellationToken ct = default);
}

public interface IZoneService
{
    Task<IReadOnlyList<Zone>> ListAsync(string ownerId, CancellationToken ct = default);

    Task<Zone> CreateAsync(string ownerId, string apex, CancellationToken ct = default);

    Task<Zone> GetAsync(string ownerId, string zoneId, CancellationToken ct = default);

    Task DeleteAsync(string ownerId, string zoneId, CancellationToken ct = default);

    Task<Zone> UpdateSoaAsync(string ownerId, string zoneId, Soa soa, CancellationToken ct = default);

    Task<DelegationCheck> CheckDelegationAsync(string ownerId, string zoneId, CancellationToken ct = default);
}

public interface IRecordService
{
    Task<IReadOnlyList<DnsRecord>> ListAsync(string ownerId, string zoneId, CancellationToken ct = default);

    Task<DnsRecord> CreateAsync(string ownerId, string zoneId, RecordDraft draft, CancellationToken ct = default);

    Task<DnsRecord> UpdateAsync(string ownerId, string zoneId, string recordId, RecordDraft draft, CancellationToken ct = default);

    Task DeleteAsync(string ownerId, string zoneId, string recordId, CancellationToken ct = default);
}

public interface IZoneSyncService
{
    Task<ZoneManifestResponse> GetManifestAsync(CancellationToken ct = default);

    Task<ZoneDocumentResponse?> GetZoneDocumentAsync(string zoneId, CancellationToken ct = default);
}

public interface IDelegationChecker
{
    Task<IReadOnlyList<string>> LookupNameserversAsync(string apex, CancellationToken ct = default);
}

public interface IHermesNotifier
{
    Task NotifyAsync(CancellationToken ct = default);
}

public interface ICognitoIdentityService
{
    Task<IReadOnlyList<string>> ListEnabledProvidersAsync(CancellationToken ct = default);
}
