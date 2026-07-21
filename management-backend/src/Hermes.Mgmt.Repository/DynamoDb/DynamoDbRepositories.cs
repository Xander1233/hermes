using Amazon.DynamoDBv2;
using Amazon.DynamoDBv2.Model;
using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Repository.DynamoDb;

public sealed class DynamoDbProfileRepository(IAmazonDynamoDB client, IOptions<DynamoDbOptions> options) : IProfileRepository
{
    private readonly string _table = options.Value.TableName;

    public async Task<UserProfile?> GetAsync(string userId, CancellationToken ct = default)
    {
        var response = await client.GetItemAsync(new GetItemRequest
        {
            TableName = _table,
            Key = DynamoDbZoneRepository.Key(TableSchema.UserPk(userId), TableSchema.ProfileSk),
            ConsistentRead = true
        }, ct);

        return response.IsItemSet ? ItemMapper.ToProfile(response.Item) : null;
    }

    public Task PutAsync(UserProfile profile, CancellationToken ct = default) =>
        client.PutItemAsync(new PutItemRequest
        {
            TableName = _table,
            Item = ItemMapper.ToItem(profile)
        }, ct);
}

public sealed class DynamoDbRecordRepository(IAmazonDynamoDB client, IOptions<DynamoDbOptions> options) : IRecordRepository
{
    private readonly string _table = options.Value.TableName;

    public async Task<IReadOnlyList<DnsRecord>> ListByZoneAsync(string zoneId, CancellationToken ct = default)
    {
        var records = new List<DnsRecord>();
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
                ConsistentRead = true,
                ExclusiveStartKey = startKey
            }, ct);

            records.AddRange(response.Items.Select(ItemMapper.ToRecord));
            startKey = response.LastEvaluatedKey is { Count: > 0 } ? response.LastEvaluatedKey : null;
        }
        while (startKey is not null);

        return records;
    }

    public async Task<DnsRecord?> GetAsync(string zoneId, string recordId, CancellationToken ct = default)
    {
        var response = await client.GetItemAsync(new GetItemRequest
        {
            TableName = _table,
            Key = DynamoDbZoneRepository.Key(TableSchema.ZonePk(zoneId), TableSchema.RecordSk(recordId)),
            ConsistentRead = true
        }, ct);

        return response.IsItemSet ? ItemMapper.ToRecord(response.Item) : null;
    }

    public Task PutAsync(DnsRecord record, CancellationToken ct = default) =>
        client.PutItemAsync(new PutItemRequest
        {
            TableName = _table,
            Item = ItemMapper.ToItem(record)
        }, ct);

    public async Task<bool> DeleteAsync(string zoneId, string recordId, CancellationToken ct = default)
    {
        try
        {
            await client.DeleteItemAsync(new DeleteItemRequest
            {
                TableName = _table,
                Key = DynamoDbZoneRepository.Key(TableSchema.ZonePk(zoneId), TableSchema.RecordSk(recordId)),
                ConditionExpression = "attribute_exists(#pk)",
                ExpressionAttributeNames = new Dictionary<string, string> { ["#pk"] = TableSchema.Pk }
            }, ct);

            return true;
        }
        catch (ConditionalCheckFailedException)
        {
            return false;
        }
    }
}
