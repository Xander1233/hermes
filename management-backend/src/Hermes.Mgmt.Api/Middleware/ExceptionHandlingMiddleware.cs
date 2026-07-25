using System.Net;
using System.Text.Json;
using Hermes.Mgmt.Common.Contracts.V1;
using Hermes.Mgmt.Common.Errors;

namespace Hermes.Mgmt.Api.Middleware;

/// <summary>
/// Turns <see cref="AppException"/> into the RFC 7807 body the dashboard binds to, and
/// everything else into an opaque 500.
/// </summary>
/// <remarks>
/// Centralized so controllers stay free of try/catch and so there is exactly one place that
/// decides what leaks to a client. Unexpected exceptions deliberately return a generic message:
/// a DynamoDB error string can carry table names and key structure.
/// </remarks>
public sealed class ExceptionHandlingMiddleware(
    RequestDelegate next,
    ILogger<ExceptionHandlingMiddleware> logger)
{
    private static readonly JsonSerializerOptions SerializerOptions = new(JsonSerializerDefaults.Web);

    public async Task InvokeAsync(HttpContext context)
    {
        try
        {
            await next(context);
        }
        catch (AppException ex)
        {
            // Expected, and part of the contract — logged at information, not error.
            logger.LogInformation(
                "Request {Method} {Path} rejected: {Status} {Title}",
                context.Request.Method, context.Request.Path, (int)ex.Status, ex.Title);

            await WriteAsync(context, new ProblemResponse(
                ex.Title,
                (int)ex.Status,
                ex.Detail,
                ex.Errors.Count > 0 ? ex.Errors : null));
        }
        catch (OperationCanceledException) when (context.RequestAborted.IsCancellationRequested)
        {
            // The client hung up. Nothing to report, and nowhere to report it to.
            logger.LogDebug("Request {Path} aborted by the client", context.Request.Path);
        }
        catch (Exception ex)
        {
            logger.LogError(ex,
                "Unhandled exception for {Method} {Path}",
                context.Request.Method, context.Request.Path);

            await WriteAsync(context, new ProblemResponse(
                "Internal server error",
                (int)HttpStatusCode.InternalServerError,
                "An unexpected error occurred.",
                null));
        }
    }

    private static async Task WriteAsync(HttpContext context, ProblemResponse problem)
    {
        if (context.Response.HasStarted)
        {
            // Too late to change the status code; overwriting now would produce a mangled body.
            return;
        }

        context.Response.Clear();
        context.Response.StatusCode = problem.Status;
        context.Response.ContentType = "application/problem+json";

        await context.Response.WriteAsync(JsonSerializer.Serialize(problem, SerializerOptions));
    }
}
