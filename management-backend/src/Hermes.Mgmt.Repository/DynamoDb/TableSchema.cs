using Amazon.DynamoDBv2.Model;

namespace Hermes.Mgmt.Repository.DynamoDb;


public static class TableSchema
{
    public const string Pk = "PK";
    public const string Sk = "SK";

    public const string Gsi1 = "GSI1";
    public const string Gsi1Pk = "GSI1PK";
    public const string Gsi1Sk = "GSI1SK";

    public const string Gsi2 = "GSI2";
    public const string Gsi2Pk = "GSI2PK";
    public const string Gsi2Sk = "GSI2SK";

    public const string Gsi3 = "GSI3";
    public const string Gsi3Pk = "GSI3PK";
    public const string Gsi3Sk = "GSI3SK";

    public const string AllZonesPartition = "ZONES";

    public const string PendingDelegationPartition = "PENDING";

    public const string ProfileSk = "PROFILE";
    public const string ZoneMetaSk = "META";
    public const string ApexClaimSk = "CLAIM";
    public const string RecordSkPrefix = "REC#";

    public static string UserPk(string userId) => $"USER#{userId}";
    public static string ZonePk(string zoneId) => $"ZONE#{zoneId}";
    public static string ApexPk(string apex) => $"APEX#{apex}";
    public static string RecordSk(string recordId) => $"{RecordSkPrefix}{recordId}";

    // dev-only
    public static CreateTableRequest CreateTableRequest(string tableName) => new()
    {
        TableName = tableName,
        BillingMode = Amazon.DynamoDBv2.BillingMode.PAY_PER_REQUEST,
        AttributeDefinitions =
        [
            new AttributeDefinition(Pk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Sk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Gsi1Pk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Gsi1Sk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Gsi2Pk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Gsi2Sk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Gsi3Pk, Amazon.DynamoDBv2.ScalarAttributeType.S),
            new AttributeDefinition(Gsi3Sk, Amazon.DynamoDBv2.ScalarAttributeType.S)
        ],
        KeySchema =
        [
            new KeySchemaElement(Pk, Amazon.DynamoDBv2.KeyType.HASH),
            new KeySchemaElement(Sk, Amazon.DynamoDBv2.KeyType.RANGE)
        ],
        GlobalSecondaryIndexes =
        [
            new GlobalSecondaryIndex
            {
                IndexName = Gsi1,
                KeySchema =
                [
                    new KeySchemaElement(Gsi1Pk, Amazon.DynamoDBv2.KeyType.HASH),
                    new KeySchemaElement(Gsi1Sk, Amazon.DynamoDBv2.KeyType.RANGE)
                ],
                Projection = new Projection { ProjectionType = Amazon.DynamoDBv2.ProjectionType.ALL }
            },
            new GlobalSecondaryIndex
            {
                IndexName = Gsi2,
                KeySchema =
                [
                    new KeySchemaElement(Gsi2Pk, Amazon.DynamoDBv2.KeyType.HASH),
                    new KeySchemaElement(Gsi2Sk, Amazon.DynamoDBv2.KeyType.RANGE)
                ],
                Projection = new Projection
                {
                    ProjectionType = Amazon.DynamoDBv2.ProjectionType.INCLUDE,
                    NonKeyAttributes = [ItemAttributes.Apex, ItemAttributes.Version]
                }
            },
            new GlobalSecondaryIndex
            {
                IndexName = Gsi3,
                KeySchema =
                [
                    new KeySchemaElement(Gsi3Pk, Amazon.DynamoDBv2.KeyType.HASH),
                    new KeySchemaElement(Gsi3Sk, Amazon.DynamoDBv2.KeyType.RANGE)
                ],
                Projection = new Projection { ProjectionType = Amazon.DynamoDBv2.ProjectionType.ALL }
            }
        ]
    };
}

public static class ItemAttributes
{
    public const string Id = "Id";
    public const string Apex = "Apex";
    public const string OwnerId = "OwnerId";
    public const string Status = "Status";
    public const string StatusMessage = "StatusMessage";
    public const string Version = "Version";
    public const string Nameservers = "Nameservers";
    public const string RecordCount = "RecordCount";
    public const string NextDelegationCheckAt = "NextDelegationCheckAt";
    public const string CreatedAt = "CreatedAt";
    public const string UpdatedAt = "UpdatedAt";

    public const string SoaMname = "SoaMname";
    public const string SoaRname = "SoaRname";
    public const string SoaSerial = "SoaSerial";
    public const string SoaRefresh = "SoaRefresh";
    public const string SoaRetry = "SoaRetry";
    public const string SoaExpire = "SoaExpire";
    public const string SoaMinimum = "SoaMinimum";
    public const string SoaTtl = "SoaTtl";

    public const string ZoneId = "ZoneId";
    public const string Name = "Name";
    public const string Type = "Type";
    public const string Ttl = "Ttl";
    public const string Data = "Data";
    public const string Preference = "Preference";
    public const string Priority = "Priority";
    public const string Weight = "Weight";
    public const string Port = "Port";
    public const string Flags = "Flags";
    public const string Tag = "Tag";
    public const string Notes = "Notes";

    public const string Email = "Email";
    public const string DisplayName = "DisplayName";
    public const string Company = "Company";
    public const string Timezone = "Timezone";
    public const string NotificationEmail = "NotificationEmail";
    public const string NotifyOnZoneErrors = "NotifyOnZoneErrors";
}
