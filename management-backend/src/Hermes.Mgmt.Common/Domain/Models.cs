namespace Hermes.Mgmt.Common.Domain;

public sealed record UserProfile
{
    public required string Id { get; init; }

    public required string Email { get; init; }

    public required string DisplayName { get; init; }

    public string? Company { get; init; }

    /// <summary>IANA timezone id, e.g. <c>Europe/Zurich</c>.</summary>
    public required string Timezone { get; init; }

    public string? NotificationEmail { get; init; }

    public bool NotifyOnZoneErrors { get; init; }

    public DateTimeOffset CreatedAt { get; init; }
}

public sealed record Soa
{
    public required string Mname { get; init; }
    public required string Rname { get; init; }
    public Serial Serial { get; init; }
    public int Refresh { get; init; }
    public int Retry { get; init; }
    public int Expire { get; init; }
    public uint Minimum { get; init; }
    public uint Ttl { get; init; }
}

public sealed record Zone
{
    public required string Id { get; init; }
    public required string Apex { get; init; }
    public required string OwnerId { get; init; }
    public ZoneStatus Status { get; init; }
    public string? StatusMessage { get; init; }
    public ulong Version { get; init; }
    public required IReadOnlyList<string> Nameservers { get; init; }
    public required Soa Soa { get; init; }
    public int RecordCount { get; init; }
    public DateTimeOffset? NextDelegationCheckAt { get; init; }
    public DateTimeOffset CreatedAt { get; init; }
    public DateTimeOffset UpdatedAt { get; init; }
}

public sealed record DnsRecord
{
    public required string Id { get; init; }
    public required string ZoneId { get; init; }
    public required string Name { get; init; }
    public RecordType Type { get; init; }
    public uint Ttl { get; init; }

    /// <summary>Primary value: address, target name, or text.</summary>
    public string? Data { get; init; }

    /// <summary>MX only.</summary>
    public ushort? Preference { get; init; }

    /// <summary>SRV only.</summary>
    public ushort? Priority { get; init; }

    /// <summary>SRV only.</summary>
    public ushort? Weight { get; init; }

    /// <summary>SRV only.</summary>
    public ushort? Port { get; init; }

    /// <summary>CAA only.</summary>
    public byte? Flags { get; init; }

    /// <summary>CAA only: <c>issue</c>, <c>issuewild</c> or <c>iodef</c>.</summary>
    public string? Tag { get; init; }

    /// <summary>
    /// Free-form user note. Management metadata only — it is never published to Hermes,
    /// because it is not DNS data.
    /// </summary>
    public string? Notes { get; init; }

    public DateTimeOffset CreatedAt { get; init; }
    public DateTimeOffset UpdatedAt { get; init; }
}

public sealed record DelegationCheck
{
    public required IReadOnlyList<string> ObservedNameservers { get; init; }
    public required IReadOnlyList<string> ExpectedNameservers { get; init; }
    public bool Delegated { get; init; }
    public DateTimeOffset CheckedAt { get; init; }
}

public sealed record ZoneRef(string Id, string Apex, ulong Version);
