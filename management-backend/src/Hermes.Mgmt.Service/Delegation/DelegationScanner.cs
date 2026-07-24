using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service.Delegation;

public sealed record ScanResult(int Examined, int Activated, int StillPending, int Skipped)
{
    public static readonly ScanResult Empty = new(0, 0, 0, 0);
}

public sealed class DelegationScanner(IZoneRepository zones, IDelegationChecker checker, ZonePublisher publisher, IOptions<DelegationOptions> options, TimeProvider clock, ILogger<DelegationScanner> logger)
{
    private readonly DelegationOptions _options = options.Value;

    public async Task<ScanResult> RunOnceAsync(CancellationToken ct = default)
    {
        if (!_options.Enabled)
        {
            return ScanResult.Empty;
        }

        var now = clock.GetUtcNow();
        var due = await zones.ListDueForDelegationCheckAsync(now, _options.BatchSize, ct);
        if (due.Count == 0)
        {
            return ScanResult.Empty;
        }

        var activated = 0;
        var stillPending = 0;
        var skipped = 0;

        foreach (var zone in due)
        {
            ct.ThrowIfCancellationRequested();

            var nextDueAt = DelegationPolicy.NextCheckAt(zone.CreatedAt, now);
            if (!await zones.TryClaimDelegationCheckAsync(zone.Id, zone.NextDelegationCheckAt, nextDueAt, ct))
            {
                skipped++;
                continue;
            }

            if (await CheckOneAsync(zone, now, ct))
            {
                activated++;
            }
            else
            {
                stillPending++;
            }
        }

        var result = new ScanResult(due.Count, activated, stillPending, skipped);

        if (activated > 0)
        {
            logger.LogInformation("Delegation scan activated {Activated} zone(s): {@Result}", activated, result);
        }
        else
        {
            logger.LogDebug("Delegation scan found nothing to activate: {@Result}", result);
        }

        return result;
    }

    private async Task<bool> CheckOneAsync(Zone zone, DateTimeOffset now, CancellationToken ct)
    {
        IReadOnlyList<string> observed;
        try
        {
            observed = await checker.LookupNameserversAsync(zone.Apex, ct);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            logger.LogDebug(ex, "Delegation lookup failed for {Apex}", zone.Apex);
            return false;
        }

        if (!DelegationPolicy.IsDelegated(zone.Nameservers, observed))
        {
            return false;
        }

        await zones.UpdateAsync(
            zone with
            {
                Status = ZoneStatus.Active,
                StatusMessage = null,
                NextDelegationCheckAt = null,
                UpdatedAt = now
            }, ct);

        logger.LogInformation("Zone {Apex} ({ZoneId}) is now delegated to {Nameservers}; activated", zone.Apex, zone.Id, string.Join(", ", zone.Nameservers));

        await publisher.PublishAsync(zone.Id, ct);
        return true;
    }
}
