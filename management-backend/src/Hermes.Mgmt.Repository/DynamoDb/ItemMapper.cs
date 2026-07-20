using System.Globalization;
using Amazon.DynamoDBv2.Model;
using Hermes.Mgmt.Common.Domain;

namespace Hermes.Mgmt.Repository.DynamoDb;

public static class ItemMapper
{
    private static AttributeValue S(string value) => new() { S = value };

    private static AttributeValue N(long value) => new() { N = value.ToString(CultureInfo.InvariantCulture) };

    private static AttributeValue N(ulong value) => new() { N = value.ToString(CultureInfo.InvariantCulture) };

    private static AttributeValue Bool(bool value) => new() { BOOL = value };

    private static AttributeValue OptS(string? value) => string.IsNullOrEmpty(value) ? new AttributeValue { NULL = true } : S(value);

    private static AttributeValue OptN(long? value) => value is null ? new AttributeValue { NULL = true } : N(value.Value);

    private static string? GetString(Dictionary<string, AttributeValue> item, string key) => item.TryGetValue(key, out var value) && value.NULL != true ? value.S : null;

    private static string GetRequiredString(Dictionary<string, AttributeValue> item, string key) => GetString(item, key) ?? throw new InvalidOperationException($"DynamoDB item is missing required attribute '{key}'.");

    private static long GetLong(Dictionary<string, AttributeValue> item, string key, long fallback = 0) =>
        item.TryGetValue(key, out var value) && value.NULL != true && value.N is not null
            ? long.Parse(value.N, CultureInfo.InvariantCulture)
            : fallback;

    private static long? GetNullableLong(Dictionary<string, AttributeValue> item, string key) =>
        item.TryGetValue(key, out var value) && value.NULL != true && value.N is not null
            ? long.Parse(value.N, CultureInfo.InvariantCulture)
            : null;

    private static bool GetBool(Dictionary<string, AttributeValue> item, string key) => item.TryGetValue(key, out var value) && value.BOOL == true;

    private static DateTimeOffset GetTimestamp(Dictionary<string, AttributeValue> item, string key)
    {
        var raw = GetString(item, key);
        
        return raw is null
            ? default
            : DateTimeOffset.Parse(raw, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind);
    }

    private static DateTimeOffset? GetNullableTimestamp(Dictionary<string, AttributeValue> item, string key)
    {
        var raw = GetString(item, key);
        
        return raw is null
            ? null
            : DateTimeOffset.Parse(raw, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind);
    }

    private static AttributeValue Timestamp(DateTimeOffset value) => S(value.ToUniversalTime().ToString("O", CultureInfo.InvariantCulture));

    public static Dictionary<string, AttributeValue> ToItem(UserProfile profile) => new()
    {
        [TableSchema.Pk] = S(TableSchema.UserPk(profile.Id)),
        [TableSchema.Sk] = S(TableSchema.ProfileSk),
        [ItemAttributes.Id] = S(profile.Id),
        [ItemAttributes.Email] = S(profile.Email),
        [ItemAttributes.DisplayName] = S(profile.DisplayName),
        [ItemAttributes.Company] = OptS(profile.Company),
        [ItemAttributes.Timezone] = S(profile.Timezone),
        [ItemAttributes.NotificationEmail] = OptS(profile.NotificationEmail),
        [ItemAttributes.NotifyOnZoneErrors] = Bool(profile.NotifyOnZoneErrors),
        [ItemAttributes.CreatedAt] = Timestamp(profile.CreatedAt)
    };

    public static UserProfile ToProfile(Dictionary<string, AttributeValue> item) => new()
    {
        Id = GetRequiredString(item, ItemAttributes.Id),
        Email = GetRequiredString(item, ItemAttributes.Email),
        DisplayName = GetRequiredString(item, ItemAttributes.DisplayName),
        Company = GetString(item, ItemAttributes.Company),
        Timezone = GetString(item, ItemAttributes.Timezone) ?? "UTC",
        NotificationEmail = GetString(item, ItemAttributes.NotificationEmail),
        NotifyOnZoneErrors = GetBool(item, ItemAttributes.NotifyOnZoneErrors),
        CreatedAt = GetTimestamp(item, ItemAttributes.CreatedAt)
    };

    public static Dictionary<string, AttributeValue> ToItem(Zone zone)
    {
        var item = ToZoneItemCore(zone);

        if (zone.Status == ZoneStatus.Pending && zone.NextDelegationCheckAt is { } dueAt)
        {
            item[TableSchema.Gsi3Pk] = S(TableSchema.PendingDelegationPartition);
            item[TableSchema.Gsi3Sk] = Timestamp(dueAt);
        }

        return item;
    }

    private static Dictionary<string, AttributeValue> ToZoneItemCore(Zone zone) => new()
    {
        [TableSchema.Pk] = S(TableSchema.ZonePk(zone.Id)),
        [TableSchema.Sk] = S(TableSchema.ZoneMetaSk),

        [TableSchema.Gsi1Pk] = S(TableSchema.UserPk(zone.OwnerId)),
        [TableSchema.Gsi1Sk] = S($"APEX#{zone.Apex}"),
        [TableSchema.Gsi2Pk] = S(TableSchema.AllZonesPartition),
        [TableSchema.Gsi2Sk] = S(zone.Id),

        [ItemAttributes.NextDelegationCheckAt] = zone.NextDelegationCheckAt is { } due ? Timestamp(due) : new AttributeValue { NULL = true },
        [ItemAttributes.Id] = S(zone.Id),
        [ItemAttributes.Apex] = S(zone.Apex),
        [ItemAttributes.OwnerId] = S(zone.OwnerId),
        [ItemAttributes.Status] = S(zone.Status.ToString()),
        [ItemAttributes.StatusMessage] = OptS(zone.StatusMessage),
        [ItemAttributes.Version] = N(zone.Version),
        [ItemAttributes.Nameservers] = new AttributeValue { SS = zone.Nameservers.ToList() },
        [ItemAttributes.RecordCount] = N(zone.RecordCount),
        [ItemAttributes.CreatedAt] = Timestamp(zone.CreatedAt),
        [ItemAttributes.UpdatedAt] = Timestamp(zone.UpdatedAt),

        [ItemAttributes.SoaMname] = S(zone.Soa.Mname),
        [ItemAttributes.SoaRname] = S(zone.Soa.Rname),
        [ItemAttributes.SoaSerial] = N(zone.Soa.Serial.Value),
        [ItemAttributes.SoaRefresh] = N(zone.Soa.Refresh),
        [ItemAttributes.SoaRetry] = N(zone.Soa.Retry),
        [ItemAttributes.SoaExpire] = N(zone.Soa.Expire),
        [ItemAttributes.SoaMinimum] = N(zone.Soa.Minimum),
        [ItemAttributes.SoaTtl] = N(zone.Soa.Ttl)
    };

    public static Zone ToZone(Dictionary<string, AttributeValue> item) => new()
    {
        Id = GetRequiredString(item, ItemAttributes.Id),
        Apex = GetRequiredString(item, ItemAttributes.Apex),
        OwnerId = GetRequiredString(item, ItemAttributes.OwnerId),
        Status = Enum.TryParse<ZoneStatus>(GetString(item, ItemAttributes.Status), out var status) ? status : ZoneStatus.Pending,
        StatusMessage = GetString(item, ItemAttributes.StatusMessage),
        Version = (ulong)GetLong(item, ItemAttributes.Version),
        Nameservers = item.TryGetValue(ItemAttributes.Nameservers, out var ns) && ns.SS is { Count: > 0 } ? ns.SS.OrderBy(n => n, StringComparer.Ordinal).ToList() : [],
        RecordCount = (int)GetLong(item, ItemAttributes.RecordCount),
        NextDelegationCheckAt = GetNullableTimestamp(item, ItemAttributes.NextDelegationCheckAt),
        CreatedAt = GetTimestamp(item, ItemAttributes.CreatedAt),
        UpdatedAt = GetTimestamp(item, ItemAttributes.UpdatedAt),
        Soa = new Soa
        {
            Mname = GetString(item, ItemAttributes.SoaMname) ?? string.Empty,
            Rname = GetString(item, ItemAttributes.SoaRname) ?? string.Empty,
            Serial = new Serial((uint)GetLong(item, ItemAttributes.SoaSerial)),
            Refresh = (int)GetLong(item, ItemAttributes.SoaRefresh),
            Retry = (int)GetLong(item, ItemAttributes.SoaRetry),
            Expire = (int)GetLong(item, ItemAttributes.SoaExpire),
            Minimum = (uint)GetLong(item, ItemAttributes.SoaMinimum),
            Ttl = (uint)GetLong(item, ItemAttributes.SoaTtl)
        }
    };

    public static ZoneRef ToZoneRef(Dictionary<string, AttributeValue> item) => new(
        Id: GetString(item, ItemAttributes.Id) ?? GetRequiredString(item, TableSchema.Gsi2Sk),
        Apex: GetRequiredString(item, ItemAttributes.Apex),
        Version: (ulong)GetLong(item, ItemAttributes.Version));

    public static Dictionary<string, AttributeValue> ToItem(DnsRecord record) => new()
    {
        [TableSchema.Pk] = S(TableSchema.ZonePk(record.ZoneId)),
        [TableSchema.Sk] = S(TableSchema.RecordSk(record.Id)),

        [ItemAttributes.Id] = S(record.Id),
        [ItemAttributes.ZoneId] = S(record.ZoneId),
        [ItemAttributes.Name] = S(record.Name),
        [ItemAttributes.Type] = S(record.Type.ToString()),
        [ItemAttributes.Ttl] = N(record.Ttl),
        [ItemAttributes.Data] = OptS(record.Data),
        [ItemAttributes.Preference] = OptN(record.Preference),
        [ItemAttributes.Priority] = OptN(record.Priority),
        [ItemAttributes.Weight] = OptN(record.Weight),
        [ItemAttributes.Port] = OptN(record.Port),
        [ItemAttributes.Flags] = OptN(record.Flags),
        [ItemAttributes.Tag] = OptS(record.Tag),
        [ItemAttributes.Notes] = OptS(record.Notes),
        [ItemAttributes.CreatedAt] = Timestamp(record.CreatedAt),
        [ItemAttributes.UpdatedAt] = Timestamp(record.UpdatedAt)
    };

    public static DnsRecord ToRecord(Dictionary<string, AttributeValue> item) => new()
    {
        Id = GetRequiredString(item, ItemAttributes.Id),
        ZoneId = GetRequiredString(item, ItemAttributes.ZoneId),
        Name = GetRequiredString(item, ItemAttributes.Name),
        Type = Enum.Parse<RecordType>(GetRequiredString(item, ItemAttributes.Type)),
        Ttl = (uint)GetLong(item, ItemAttributes.Ttl),
        Data = GetString(item, ItemAttributes.Data),
        Preference = (ushort?)GetNullableLong(item, ItemAttributes.Preference),
        Priority = (ushort?)GetNullableLong(item, ItemAttributes.Priority),
        Weight = (ushort?)GetNullableLong(item, ItemAttributes.Weight),
        Port = (ushort?)GetNullableLong(item, ItemAttributes.Port),
        Flags = (byte?)GetNullableLong(item, ItemAttributes.Flags),
        Tag = GetString(item, ItemAttributes.Tag),
        Notes = GetString(item, ItemAttributes.Notes),
        CreatedAt = GetTimestamp(item, ItemAttributes.CreatedAt),
        UpdatedAt = GetTimestamp(item, ItemAttributes.UpdatedAt)
    };

    public static Dictionary<string, AttributeValue> ToApexClaimItem(string apex, string zoneId) => new()
    {
        [TableSchema.Pk] = S(TableSchema.ApexPk(apex)),
        [TableSchema.Sk] = S(TableSchema.ApexClaimSk),
        [ItemAttributes.Apex] = S(apex),
        [ItemAttributes.ZoneId] = S(zoneId)
    };
}
