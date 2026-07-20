using System.ComponentModel.DataAnnotations;

namespace Hermes.Mgmt.Common.Contracts.V1;

public sealed record ProfileResponse(
    string Id,
    string Email,
    string DisplayName,
    string? Company,
    string Timezone,
    string? NotificationEmail,
    bool NotifyOnZoneErrors,
    DateTimeOffset CreatedAt);

public sealed record UpdateProfileRequest
{
    [Required(AllowEmptyStrings = false)]
    [StringLength(100, MinimumLength = 1)]
    public string DisplayName { get; init; } = string.Empty;

    [StringLength(200)]
    public string? Company { get; init; }

    [Required(AllowEmptyStrings = false)]
    public string Timezone { get; init; } = string.Empty;

    [EmailAddress]
    public string? NotificationEmail { get; init; }

    public bool NotifyOnZoneErrors { get; init; }
}

public sealed record SoaModel
{
    [Required(AllowEmptyStrings = false)]
    public string Mname { get; init; } = string.Empty;

    [Required(AllowEmptyStrings = false)]
    public string Rname { get; init; } = string.Empty;

    public uint Serial { get; init; }

    public int Refresh { get; init; }
    public int Retry { get; init; }
    public int Expire { get; init; }
    public uint Minimum { get; init; }
    public uint Ttl { get; init; }
}

public sealed record ZoneResponse(
    string Id,
    string Apex,
    string Status,
    string? StatusMessage,
    ulong Version,
    IReadOnlyList<string> Nameservers,
    int RecordCount,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt);

public sealed record ZoneDetailResponse(
    string Id,
    string Apex,
    string Status,
    string? StatusMessage,
    ulong Version,
    IReadOnlyList<string> Nameservers,
    int RecordCount,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt,
    SoaModel Soa);

public sealed record CreateZoneRequest
{
    [Required(AllowEmptyStrings = false)]
    public string Apex { get; init; } = string.Empty;
}

public sealed record DelegationCheckResponse(
    IReadOnlyList<string> ObservedNameservers,
    IReadOnlyList<string> ExpectedNameservers,
    bool Delegated,
    DateTimeOffset CheckedAt);

public sealed record RecordResponse(
    string Id,
    string ZoneId,
    string Name,
    string Type,
    uint Ttl,
    string? Data,
    ushort? Preference,
    ushort? Priority,
    ushort? Weight,
    ushort? Port,
    byte? Flags,
    string? Tag,
    string? Notes,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt);

public sealed record RecordRequest
{
    [Required(AllowEmptyStrings = false)]
    public string Name { get; init; } = string.Empty;

    [Required(AllowEmptyStrings = false)]
    public string Type { get; init; } = string.Empty;

    public uint Ttl { get; init; }

    public string? Data { get; init; }

    public ushort? Preference { get; init; }
    public ushort? Priority { get; init; }
    public ushort? Weight { get; init; }
    public ushort? Port { get; init; }
    public byte? Flags { get; init; }
    public string? Tag { get; init; }

    [StringLength(1000)]
    public string? Notes { get; init; }
}

public sealed record ProblemResponse(
    string Title,
    int Status,
    string? Detail,
    IReadOnlyDictionary<string, string[]>? Errors);
