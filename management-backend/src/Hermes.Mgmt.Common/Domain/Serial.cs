namespace Hermes.Mgmt.Common.Domain;

public readonly record struct Serial(uint Value)
{
    public static Serial FromTimestamp(DateTimeOffset now) =>
        new((uint)now.ToUnixTimeSeconds());
    
    public Serial Next(DateTimeOffset now)
    {
        if (Value == uint.MaxValue)
        {
            return this;
        }

        var unix = (uint)now.ToUnixTimeSeconds();
        
        return new Serial(unix > Value ? unix : Value + 1);
    }
}
