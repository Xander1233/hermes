using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Common.Errors;
using Hermes.Mgmt.Service.Options;
using Hermes.Mgmt.Service.Validation;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service;

public sealed class RecordService(IZoneRepository zones, IRecordRepository records, ZonePublisher publisher, IOptions<ZoneDefaultsOptions> defaultsOptions, TimeProvider clock) : IRecordService
{
    private readonly ZoneDefaultsOptions _defaults = defaultsOptions.Value;

    public async Task<IReadOnlyList<DnsRecord>> ListAsync(string ownerId, string zoneId, CancellationToken ct = default)
    {
        var zone = await LoadOwnedZoneAsync(ownerId, zoneId, ct);
        var all = await records.ListByZoneAsync(zone.Id, ct);

        return all
            .OrderBy(r => r.Name, StringComparer.Ordinal)
            .ThenBy(r => r.Type)
            .ToList();
    }

    public async Task<DnsRecord> CreateAsync(string ownerId, string zoneId, RecordDraft draft, CancellationToken ct = default)
    {
        var zone = await LoadOwnedZoneAsync(ownerId, zoneId, ct);
        var existing = await records.ListByZoneAsync(zone.Id, ct);

        if (_defaults.MaxRecordsPerZone > 0 && existing.Count >= _defaults.MaxRecordsPerZone)
        {
            throw AppException.Validation("name", $"This zone has reached the limit of {_defaults.MaxRecordsPerZone} records.");
        }

        var normalized = Normalize(draft, zone.Apex);
        Validate(normalized, zone.Apex, existing, ignoreRecordId: null);

        var now = clock.GetUtcNow();
        var record = ToRecord(normalized, zone.Id, NewRecordId(), now, now);

        await records.PutAsync(record, ct);
        await zones.BumpVersionAsync(zone.Id, recordCountDelta: +1, now, ct);
        await publisher.PublishAsync(zone.Id, ct);

        return record;
    }

    public async Task<DnsRecord> UpdateAsync(string ownerId, string zoneId, string recordId, RecordDraft draft, CancellationToken ct = default)
    {
        var zone = await LoadOwnedZoneAsync(ownerId, zoneId, ct);

        var current = await records.GetAsync(zone.Id, recordId, ct) ?? throw AppException.NotFound("Record");

        var existing = await records.ListByZoneAsync(zone.Id, ct);
        var normalized = Normalize(draft, zone.Apex);
        Validate(normalized, zone.Apex, existing, ignoreRecordId: recordId);

        var now = clock.GetUtcNow();
        var record = ToRecord(normalized, zone.Id, current.Id, current.CreatedAt, now);

        await records.PutAsync(record, ct);
        await zones.BumpVersionAsync(zone.Id, recordCountDelta: 0, now, ct);
        await publisher.PublishAsync(zone.Id, ct);

        return record;
    }

    public async Task DeleteAsync(string ownerId, string zoneId, string recordId, CancellationToken ct = default)
    {
        var zone = await LoadOwnedZoneAsync(ownerId, zoneId, ct);

        if (!await records.DeleteAsync(zone.Id, recordId, ct))
        {
            throw AppException.NotFound("Record");
        }

        var now = clock.GetUtcNow();
        await zones.BumpVersionAsync(zone.Id, recordCountDelta: -1, now, ct);
        await publisher.PublishAsync(zone.Id, ct);
    }

    private static RecordDraft Normalize(RecordDraft draft, string apex)
    {
        var name = DnsName.ToFqdn(draft.Name, apex);
        var type = DnsValidator.NormalizeType(draft.Type, name, apex);

        return new RecordDraft
        {
            Name = name,
            Type = type,
            Ttl = draft.Ttl,
            Data = NormalizeData(draft.Data, type),
            Preference = type == RecordType.MX ? draft.Preference : null,
            Priority = type == RecordType.SRV ? draft.Priority : null,
            Weight = type == RecordType.SRV ? draft.Weight : null,
            Port = type == RecordType.SRV ? draft.Port : null,
            Flags = type == RecordType.CAA ? draft.Flags ?? 0 : null,
            Tag = type == RecordType.CAA ? draft.Tag?.Trim().ToLowerInvariant() : null,
            Notes = string.IsNullOrWhiteSpace(draft.Notes) ? null : draft.Notes.Trim()
        };
    }

    private static string? NormalizeData(string? data, RecordType type)
    {
        if (data is null)
        {
            return null;
        }

        return type switch
        {
            RecordType.CNAME or RecordType.ALIAS or RecordType.NS or RecordType.PTR or RecordType.MX or RecordType.SRV => DnsName.Normalize(data),
            RecordType.A or RecordType.AAAA => data.Trim(),
            _ => data
        };
    }

    private static void Validate(RecordDraft draft, string apex, IReadOnlyList<DnsRecord> existing, string? ignoreRecordId)
    {
        var errors = DnsValidator.Validate(draft, apex);
        if (errors.Count > 0)
        {
            throw AppException.Validation(errors);
        }

        var others = ignoreRecordId is null ? existing : existing.Where(r => r.Id != ignoreRecordId).ToList();

        var conflicts = DnsValidator.ValidateAgainstZone(draft, others);
        if (conflicts.Count > 0)
        {
            throw new AppException(System.Net.HttpStatusCode.Conflict, conflicts.ContainsKey("data") ? "Duplicate record" : "Conflicting record", conflicts.Values.First().First(), conflicts);
        }
    }

    private static DnsRecord ToRecord(RecordDraft draft, string zoneId, string id, DateTimeOffset createdAt, DateTimeOffset updatedAt) =>
        new()
        {
            Id = id,
            ZoneId = zoneId,
            Name = draft.Name,
            Type = draft.Type,
            Ttl = draft.Ttl,
            Data = draft.Data,
            Preference = draft.Preference,
            Priority = draft.Priority,
            Weight = draft.Weight,
            Port = draft.Port,
            Flags = draft.Flags,
            Tag = draft.Tag,
            Notes = draft.Notes,
            CreatedAt = createdAt,
            UpdatedAt = updatedAt
        };

    private async Task<Zone> LoadOwnedZoneAsync(string ownerId, string zoneId, CancellationToken ct)
    {
        var zone = await zones.GetAsync(zoneId, ct);
        if (zone is null || !string.Equals(zone.OwnerId, ownerId, StringComparison.Ordinal))
        {
            throw AppException.NotFound("Zone");
        }

        return zone;
    }

    internal static string NewRecordId() => $"rec_{Guid.NewGuid():N}"[..16];
}
