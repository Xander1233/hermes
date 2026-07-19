using System.Net;

namespace Hermes.Mgmt.Common.Errors;

public class AppException : Exception
{
    public AppException(HttpStatusCode status, string title, string? detail = null, IReadOnlyDictionary<string, string[]>? errors = null) : base(detail ?? title)
    {
        Status = status;
        Title = title;
        Detail = detail;
        Errors = errors ?? new Dictionary<string, string[]>();
    }

    public HttpStatusCode Status { get; }
    public string Title { get; }
    public string? Detail { get; }
    public IReadOnlyDictionary<string, string[]> Errors { get; }

    public static AppException NotFound(string what) => new(HttpStatusCode.NotFound, $"{what} not found");

    public static AppException Validation(IReadOnlyDictionary<string, string[]> errors) => new(HttpStatusCode.UnprocessableEntity, "Validation failed", "One or more fields are invalid.", errors);

    public static AppException Validation(string field, string message) => Validation(new Dictionary<string, string[]> { [field] = [message] });

    public static AppException Conflict(string title, string field, string message) => new(HttpStatusCode.Conflict, title, message, new Dictionary<string, string[]> { [field] = [message] });
}
