using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Contracts.Sync.V1;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service;

public sealed class ZoneSyncService(IZoneRepository zones, IRecordRepository records, ICacheProvider cache, IOptions<CacheOptions> cacheOptions) : IZoneSyncService
{
    private readonly TimeSpan _ttl = TimeSpan.FromSeconds(Math.Max(1, cacheOptions.Value.TtlSeconds));

    public async Task<ZoneManifestResponse> GetManifestAsync(CancellationToken ct = default)
    {
        var cached = await cache.GetAsync<ZoneManifestResponse>(CacheKeys.Manifest, ct);
        if (cached is not null)
        {
            return cached;
        }

        var refs = await zones.ListAllRefsAsync(ct);
        var manifest = new ZoneManifestResponse(
            Version: AggregateVersion(refs),
            Zones: refs
                .OrderBy(r => r.Id, StringComparer.Ordinal)
                .Select(r => new ZoneRefResponse(r.Id, r.Apex, r.Version))
                .ToList());

        await cache.SetAsync(CacheKeys.Manifest, manifest, _ttl, ct);
        return manifest;
    }

    public async Task<ZoneDocumentResponse?> GetZoneDocumentAsync(string zoneId, CancellationToken ct = default)
    {
        var key = CacheKeys.ZoneDocument(zoneId);

        var cached = await cache.GetAsync<ZoneDocumentResponse>(key, ct);
        if (cached is not null)
        {
            return cached;
        }

        var zone = await zones.GetAsync(zoneId, ct);
        if (zone is null)
        {
            return null;
        }

        var zoneRecords = await records.ListByZoneAsync(zoneId, ct);
        var document = ToDocument(zone, zoneRecords);
        
        await cache.SetAsync(key, document, _ttl, ct);
        return document;
    }

    internal static ulong AggregateVersion(IReadOnlyList<ZoneRef> refs)
    {
        ulong total = 0;
        foreach (var reference in refs)
        {
            unchecked
            {
                total += reference.Version;
            }
        }

        return total;
    }

    internal static ZoneDocumentResponse ToDocument(Zone zone, IReadOnlyList<DnsRecord> records) =>
        new(
            Id: zone.Id,
            Apex: zone.Apex,
            Version: zone.Version,
            Soa: new SoaConfigModel(
                zone.Soa.Mname,
                zone.Soa.Rname,
                zone.Soa.Serial.Value,
                zone.Soa.Refresh,
                zone.Soa.Retry,
                zone.Soa.Expire,
                zone.Soa.Minimum,
                zone.Soa.Ttl),
            Records: records
                .OrderBy(r => r.Name, StringComparer.Ordinal)
                .ThenBy(r => r.Type)
                .Select(r => new RecordConfigModel
                {
                    Name = r.Name,
                    Type = r.Type.ToString(),
                    Ttl = r.Ttl,
                    Data = r.Data,
                    Preference = r.Preference,
                    Priority = r.Priority,
                    Weight = r.Weight,
                    Port = r.Port,
                    Flags = r.Flags,
                    Tag = r.Tag
                })
                .ToList());
}
