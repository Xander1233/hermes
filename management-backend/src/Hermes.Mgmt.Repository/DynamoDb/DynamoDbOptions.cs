namespace Hermes.Mgmt.Repository.DynamoDb;

public sealed class DynamoDbOptions
{
    public const string Section = "DynamoDb";
    public string TableName { get; set; } = "hermes-mgmt";
    public string? ServiceUrl { get; set; }
    public string? Region { get; set; }
    // dev-only
    public bool CreateTableIfMissing { get; set; }
}
