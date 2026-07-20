namespace Hermes.Mgmt.Common.Contracts.Sync.V1;

public sealed record ZoneManifestResponse(ulong? Version, IReadOnlyList<ZoneRefResponse> Zones);

public sealed record ZoneRefResponse(string Id, string Apex, ulong Version);

public sealed record ZoneDocumentResponse(
    string Id,
    string Apex,
    ulong Version,
    SoaConfigModel Soa,
    IReadOnlyList<RecordConfigModel> Records);

public sealed record SoaConfigModel(
    string Mname,
    string Rname,
    uint Serial,
    int Refresh,
    int Retry,
    int Expire,
    uint Minimum,
    uint Ttl);

public sealed record RecordConfigModel
{
    public required string Name { get; init; }
    public required string Type { get; init; }
    public uint Ttl { get; init; }
    public string? Data { get; init; }
    public ushort? Preference { get; init; }
    public ushort? Priority { get; init; }
    public ushort? Weight { get; init; }
    public ushort? Port { get; init; }
    public byte? Flags { get; init; }
    public string? Tag { get; init; }
}
