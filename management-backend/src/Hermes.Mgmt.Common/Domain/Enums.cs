namespace Hermes.Mgmt.Common.Domain;

public enum ZoneStatus
{
    Pending,
    Active,
    Error
}

public enum RecordType
{
    A,
    AAAA,
    CNAME,
    /// <summary>
    /// zone-apex CNAME entry
    /// </summary>
    ALIAS,
    NS,
    MX,
    TXT,
    SRV,
    CAA,
    PTR
}
