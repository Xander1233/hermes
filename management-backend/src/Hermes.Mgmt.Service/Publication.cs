using Hermes.Mgmt.Common.Abstractions;
using Microsoft.Extensions.Logging;

namespace Hermes.Mgmt.Service;

public sealed class ZonePublisher(ICacheProvider cache, IHermesNotifier notifier, ILogger<ZonePublisher> logger)
{
    public async Task PublishAsync(string zoneId, CancellationToken ct = default)
    {
        try
        {
            await cache.RemoveAsync(CacheKeys.ZoneDocument(zoneId), ct);
            await cache.RemoveAsync(CacheKeys.Manifest, ct);
        }
        catch (Exception ex)
        {
            logger.LogWarning(ex, "Cache invalidation failed for zone {ZoneId}; it will go stale for at most the cache TTL", zoneId);
        }

        try
        {
            await notifier.NotifyAsync(ct);
        }
        catch (Exception ex)
        {
            logger.LogWarning(ex, "Notifying the Hermes fleet failed; the change propagates on the next poll instead");
        }
    }
}
