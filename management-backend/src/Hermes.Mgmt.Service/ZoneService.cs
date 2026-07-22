using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Common.Errors;
using Hermes.Mgmt.Service.Delegation;
using Hermes.Mgmt.Service.Options;
using Hermes.Mgmt.Service.Validation;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service;

public sealed class ZoneService(IZoneRepository zones, IDelegationChecker delegation, ZonePublisher publisher, IOptions<ZoneDefaultsOptions> defaultsOptions, TimeProvider clock) : IZoneService
{
    private readonly ZoneDefaultsOptions _defaults = defaultsOptions.Value;

    public Task<IReadOnlyList<Zone>> ListAsync(string ownerId, CancellationToken ct = default) => zones.ListByOwnerAsync(ownerId, ct);

    public async Task<Zone> CreateAsync(string ownerId, string apex, CancellationToken ct = default)
    {
        var normalized = DnsName.Normalize(apex);
        if (!DnsName.IsValidApex(normalized))
        {
            throw AppException.Validation("apex", "Not a valid domain name.");
        }

        if (_defaults.MaxZonesPerUser > 0)
        {
            var owned = await zones.ListByOwnerAsync(ownerId, ct);
            if (owned.Count >= _defaults.MaxZonesPerUser)
            {
                throw AppException.Validation("apex", $"You have reached the limit of {_defaults.MaxZonesPerUser} zones.");
            }
        }

        var now = clock.GetUtcNow();
        var zone = new Zone
        {
            Id = NewZoneId(),
            Apex = normalized,
            OwnerId = ownerId,
            Status = ZoneStatus.Pending,
            StatusMessage = null,
            Version = 1,
            Nameservers = _defaults.Nameservers,
            Soa = DefaultSoa(normalized, now),
            RecordCount = 0,
            NextDelegationCheckAt = now,
            CreatedAt = now,
            UpdatedAt = now
        };

        if (!await zones.TryCreateAsync(zone, ct))
        {
            throw AppException.Conflict("Zone already exists", "apex", "This zone is already managed here.");
        }

        await publisher.PublishAsync(zone.Id, ct);
        
        return zone;
    }

    public async Task<Zone> GetAsync(string ownerId, string zoneId, CancellationToken ct = default) => await LoadOwnedAsync(ownerId, zoneId, ct);

    public async Task DeleteAsync(string ownerId, string zoneId, CancellationToken ct = default)
    {
        var zone = await LoadOwnedAsync(ownerId, zoneId, ct);
        await zones.DeleteAsync(zone.Id, zone.Apex, ct);
        await publisher.PublishAsync(zone.Id, ct);
    }

    public async Task<Zone> UpdateSoaAsync(string ownerId, string zoneId, Soa soa, CancellationToken ct = default)
    {
        var zone = await LoadOwnedAsync(ownerId, zoneId, ct);

        var candidate = soa with
        {
            Mname = DnsName.Normalize(soa.Mname),
            Rname = DnsName.Normalize(soa.Rname),
            Serial = zone.Soa.Serial.Next(clock.GetUtcNow())
        };

        var errors = DnsValidator.ValidateSoa(candidate);
        if (errors.Count > 0)
        {
            throw AppException.Validation(errors);
        }

        var updated = await zones.UpdateAsync(zone with { Soa = candidate, UpdatedAt = clock.GetUtcNow() }, ct);
        await publisher.PublishAsync(zone.Id, ct);
        
        return updated;
    }

    public async Task<DelegationCheck> CheckDelegationAsync(string ownerId, string zoneId, CancellationToken ct = default)
    {
        var zone = await LoadOwnedAsync(ownerId, zoneId, ct);
        var observed = await delegation.LookupNameserversSafeAsync(zone.Apex, ct);

        var expected = zone.Nameservers;
        var delegated = DelegationPolicy.IsDelegated(expected, observed);

        if (delegated && zone.Status == ZoneStatus.Pending)
        {
            await zones.UpdateAsync(zone with
                {
                    Status = ZoneStatus.Active,
                    StatusMessage = null,
                    NextDelegationCheckAt = null,
                    UpdatedAt = clock.GetUtcNow()
                }, ct);

            await publisher.PublishAsync(zone.Id, ct);
        }

        return new DelegationCheck
        {
            ObservedNameservers = observed,
            ExpectedNameservers = expected,
            Delegated = delegated,
            CheckedAt = clock.GetUtcNow()
        };
    }

    private async Task<Zone> LoadOwnedAsync(string ownerId, string zoneId, CancellationToken ct)
    {
        var zone = await zones.GetAsync(zoneId, ct);
        if (zone is null || !string.Equals(zone.OwnerId, ownerId, StringComparison.Ordinal))
        {
            throw AppException.NotFound("Zone");
        }

        return zone;
    }

    private Soa DefaultSoa(string apex, DateTimeOffset now) => new()
    {
        Mname = _defaults.SoaMname ?? _defaults.Nameservers.FirstOrDefault() ?? $"ns1.{apex}",
        Rname = $"{_defaults.SoaRnamePrefix}.{apex}",
        Serial = Serial.FromTimestamp(now),
        Refresh = _defaults.Refresh,
        Retry = _defaults.Retry,
        Expire = _defaults.Expire,
        Minimum = _defaults.Minimum,
        Ttl = _defaults.SoaTtl
    };

    internal static string NewZoneId() => $"zone_{Guid.NewGuid():N}"[..17];
}

file static class DelegationCheckerExtensions
{
    public static async Task<IReadOnlyList<string>> LookupNameserversSafeAsync(this IDelegationChecker checker, string apex, CancellationToken ct)
    {
        try
        {
            return await checker.LookupNameserversAsync(apex, ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception)
        {
            return [];
        }
    }
}
