using System.Globalization;
using Amazon.DynamoDBv2;
using Amazon.DynamoDBv2.Model;
using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Repository.DynamoDb;

public sealed class DynamoDbZoneRepository(IAmazonDynamoDB client,IOptions<DynamoDbOptions> options) : IZoneRepository
{
    private readonly string _table = options.Value.TableName;

    public async Task<Zone?> GetAsync(string zoneId, CancellationToken ct = default)
    {
        var response = await client.GetItemAsync(new GetItemRequest
        {
            TableName = _table,
            Key = Key(TableSchema.ZonePk(zoneId), TableSchema.ZoneMetaSk),
            ConsistentRead = true
        }, ct);

        return response.IsItemSet ? ItemMapper.ToZone(response.Item) : null;
    }

    public async Task<IReadOnlyList<Zone>> ListByOwnerAsync(string ownerId, CancellationToken ct = default)
    {
        var zones = new List<Zone>();
        Dictionary<string, AttributeValue>? startKey = null;

        do
        {
            var response = await client.QueryAsync(new QueryRequest
            {
                TableName = _table,
                IndexName = TableSchema.Gsi1,
                KeyConditionExpression = "#pk = :pk",
                ExpressionAttributeNames = new Dictionary<string, string> { ["#pk"] = TableSchema.Gsi1Pk },
                ExpressionAttributeValues = new Dictionary<string, AttributeValue>
                {
                    [":pk"] = new() { S = TableSchema.UserPk(ownerId) }
                },
                ExclusiveStartKey = startKey
            }, ct);

            zones.AddRange(response.Items.Select(ItemMapper.ToZone));
            startKey = response.LastEvaluatedKey is { Count: > 0 } ? response.LastEvaluatedKey : null;
        }
        while (startKey is not null);

        return zones.OrderBy(z => z.Apex, StringComparer.Ordinal).ToList();
    }

    public async Task<IReadOnlyList<ZoneRef>> ListAllRefsAsync(CancellationToken ct = default)
    {
        var refs = new List<ZoneRef>();
        Dictionary<string, AttributeValue>? startKey = null;

        do
        {
            var response = await client.QueryAsync(new QueryRequest
            {
                TableName = _table,
                IndexName = TableSchema.Gsi2,
                KeyConditionExpression = "#pk = :pk",
                ExpressionAttributeNames = new Dictionary<string, string> { ["#pk"] = TableSchema.Gsi2Pk },
                ExpressionAttributeValues = new Dictionary<string, AttributeValue>
                {
                    [":pk"] = new() { S = TableSchema.AllZonesPartition }
                },
                ExclusiveStartKey = startKey
            }, ct);

            refs.AddRange(response.Items.Select(ItemMapper.ToZoneRef));
            startKey = response.LastEvaluatedKey is { Count: > 0 } ? response.LastEvaluatedKey : null;
        }
        while (startKey is not null);

        return refs;
    }

    public async Task<bool> TryCreateAsync(Zone zone, CancellationToken ct = default)
    {
        try
        {
            await client.TransactWriteItemsAsync(new TransactWriteItemsRequest
            {
                TransactItems =
                [
                    new TransactWriteItem
                    {
                        Put = new Put
                        {
                            TableName = _table,
                            Item = ItemMapper.ToApexClaimItem(zone.Apex, zone.Id),
                            ConditionExpression = "attribute_not_exists(#pk)",
                            ExpressionAttributeNames = new Dictionary<string, string>
                            {
                                ["#pk"] = TableSchema.Pk
                            }
                        }
                    },
                    new TransactWriteItem
                    {
                        Put = new Put
                        {
                            TableName = _table,
                            Item = ItemMapper.ToItem(zone),
                            ConditionExpression = "attribute_not_exists(#pk)",
                            ExpressionAttributeNames = new Dictionary<string, string>
                            {
                                ["#pk"] = TableSchema.Pk
                            }
                        }
                    }
                ]
            }, ct);

            return true;
        }
        catch (TransactionCanceledException ex) when (ex.CancellationReasons.Any(r => r.Code == "ConditionalCheckFailed"))
        {
            return false;
        }
    }

    public async Task<Zone> UpdateAsync(Zone zone, CancellationToken ct = default)
    {
        var updated = zone with { Version = zone.Version + 1 };

        await client.PutItemAsync(new PutItemRequest
        {
            TableName = _table,
            Item = ItemMapper.ToItem(updated),
            ConditionExpression = "attribute_exists(#pk)",
            ExpressionAttributeNames = new Dictionary<string, string> { ["#pk"] = TableSchema.Pk }
        }, ct);

        return updated;
    }

    public async Task<ulong?> BumpVersionAsync(string zoneId, int recordCountDelta, DateTimeOffset now, CancellationToken ct = default)
    {
        try
        {
            var response = await client.UpdateItemAsync(new UpdateItemRequest
            {
                TableName = _table,
                Key = Key(TableSchema.ZonePk(zoneId), TableSchema.ZoneMetaSk),
                UpdateExpression =
                    "ADD #version :one, #serial :one, #count :delta SET #updated = :now",
                ConditionExpression = "attribute_exists(#pk)",
                ExpressionAttributeNames = new Dictionary<string, string>
                {
                    ["#pk"] = TableSchema.Pk,
                    ["#version"] = ItemAttributes.Version,
                    ["#serial"] = ItemAttributes.SoaSerial,
                    ["#count"] = ItemAttributes.RecordCount,
                    ["#updated"] = ItemAttributes.UpdatedAt
                },
                ExpressionAttributeValues = new Dictionary<string, AttributeValue>
                {
                    [":one"] = new() { N = "1" },
                    [":delta"] = new() { N = recordCountDelta.ToString(CultureInfo.InvariantCulture) },
                    [":now"] = new() { S = now.ToUniversalTime().ToString("O", CultureInfo.InvariantCulture) }
                },
                ReturnValues = ReturnValue.UPDATED_NEW
            }, ct);

            return response.Attributes.TryGetValue(ItemAttributes.Version, out var version) && version.N is not null
                ? ulong.Parse(version.N, CultureInfo.InvariantCulture)
                : null;
        }
        catch (ConditionalCheckFailedException)
        {
            return null;
        }
    }

    public async Task<IReadOnlyList<Zone>> ListDueForDelegationCheckAsync(DateTimeOffset now, int limit, CancellationToken ct = default)
    {
        var response = await client.QueryAsync(new QueryRequest
        {
            TableName = _table,
            IndexName = TableSchema.Gsi3,
            KeyConditionExpression = "#pk = :pk AND #sk <= :now",
            ExpressionAttributeNames = new Dictionary<string, string>
            {
                ["#pk"] = TableSchema.Gsi3Pk,
                ["#sk"] = TableSchema.Gsi3Sk
            },
            ExpressionAttributeValues = new Dictionary<string, AttributeValue>
            {
                [":pk"] = new() { S = TableSchema.PendingDelegationPartition },
                [":now"] = new() { S = now.ToUniversalTime().ToString("O", CultureInfo.InvariantCulture) }
            },
            ScanIndexForward = true,
            Limit = limit
        }, ct);

        return response.Items.Select(ItemMapper.ToZone).ToList();
    }

    public async Task<bool> TryClaimDelegationCheckAsync(string zoneId, DateTimeOffset? expectedDueAt, DateTimeOffset nextDueAt, CancellationToken ct = default)
    {
        var names = new Dictionary<string, string>
        {
            ["#pk"] = TableSchema.Pk,
            ["#due"] = ItemAttributes.NextDelegationCheckAt,
            ["#gsi3sk"] = TableSchema.Gsi3Sk,
            ["#status"] = ItemAttributes.Status
        };

        var values = new Dictionary<string, AttributeValue>
        {
            [":next"] = new() { S = nextDueAt.ToUniversalTime().ToString("O", CultureInfo.InvariantCulture) },
            [":pending"] = new() { S = nameof(ZoneStatus.Pending) }
        };

        var condition = "attribute_exists(#pk) AND #status = :pending AND ";
        if (expectedDueAt is { } expected)
        {
            condition += "#due = :expected";
            values[":expected"] = new AttributeValue
            {
                S = expected.ToUniversalTime().ToString("O", CultureInfo.InvariantCulture)
            };
        }
        else
        {
            condition += "attribute_not_exists(#due)";
        }

        try
        {
            await client.UpdateItemAsync(new UpdateItemRequest
            {
                TableName = _table,
                Key = Key(TableSchema.ZonePk(zoneId), TableSchema.ZoneMetaSk),
                UpdateExpression = "SET #due = :next, #gsi3sk = :next",
                ConditionExpression = condition,
                ExpressionAttributeNames = names,
                ExpressionAttributeValues = values
            }, ct);

            return true;
        }
        catch (ConditionalCheckFailedException)
        {
            return false;
        }
    }

    public async Task DeleteAsync(string zoneId, string apex, CancellationToken ct = default)
    {
        var keys = await ListRecordKeysAsync(zoneId, ct);
        foreach (var batch in keys.Chunk(25))
        {
            await client.BatchWriteItemAsync(new BatchWriteItemRequest
            {
                RequestItems = new Dictionary<string, List<WriteRequest>>
                {
                    [_table] = batch
                        .Select(key => new WriteRequest { DeleteRequest = new DeleteRequest { Key = key } })
                        .ToList()
                }
            }, ct);
        }

        await client.TransactWriteItemsAsync(new TransactWriteItemsRequest
        {
            TransactItems =
            [
                new TransactWriteItem
                {
                    Delete = new Delete
                    {
                        TableName = _table,
                        Key = Key(TableSchema.ZonePk(zoneId), TableSchema.ZoneMetaSk)
                    }
                },
                new TransactWriteItem
                {
                    Delete = new Delete
                    {
                        TableName = _table,
                        Key = Key(TableSchema.ApexPk(apex), TableSchema.ApexClaimSk)
                    }
                }
            ]
        }, ct);
    }

    private async Task<List<Dictionary<string, AttributeValue>>> ListRecordKeysAsync(string zoneId, CancellationToken ct)
    {
        var keys = new List<Dictionary<string, AttributeValue>>();
        Dictionary<string, AttributeValue>? startKey = null;

        do
        {
            var response = await client.QueryAsync(new QueryRequest
            {
                TableName = _table,
                KeyConditionExpression = "#pk = :pk AND begins_with(#sk, :prefix)",
                ExpressionAttributeNames = new Dictionary<string, string>
                {
                    ["#pk"] = TableSchema.Pk,
                    ["#sk"] = TableSchema.Sk
                },
                ExpressionAttributeValues = new Dictionary<string, AttributeValue>
                {
                    [":pk"] = new() { S = TableSchema.ZonePk(zoneId) },
                    [":prefix"] = new() { S = TableSchema.RecordSkPrefix }
                },
                ProjectionExpression = "#pk, #sk",
                ExclusiveStartKey = startKey
            }, ct);

            keys.AddRange(response.Items);
            startKey = response.LastEvaluatedKey is { Count: > 0 } ? response.LastEvaluatedKey : null;
        }
        while (startKey is not null);

        return keys;
    }

    internal static Dictionary<string, AttributeValue> Key(string pk, string sk) => new()
    {
        [TableSchema.Pk] = new AttributeValue { S = pk },
        [TableSchema.Sk] = new AttributeValue { S = sk }
    };
}
