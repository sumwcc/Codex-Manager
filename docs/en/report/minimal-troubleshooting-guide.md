# Minimal Troubleshooting Manual

This manual is used to quickly locate the most common startup, forwarding and model refresh problems.

## What to look at first

1. First read [Operation and Deployment Guide](runtime-and-deployment-guide.md).
2. Read again [FAQ and Account Hitting Rules](faq-and-account-routing-rules.md).

## Common troubleshooting order

### 1. Whether the service is started

- Check whether the desktop or service is started.
- Confirm whether the listening address, port and proxy configuration are correct.
- Check the logs to see if there are any database connection failures, port occupation or permission issues.

### 2. Whether the request enters the gateway

- Confirm that the client request actually goes to the current gateway address.
- Check whether there is a corresponding path and status code in the request log.
- If it is a third-party client, confirm whether the API Key and base URL are configured correctly.

### 3. Is the model or account available?

- Check whether the account status is disabled, expired or hits limits.
- Refresh your usage and try again.
- If the account is banned, first go to the account clearing entrance.

### 4. Streaming or tools exception

- Check whether `tool_calls`, SSE end events and field restoration are lost.

## What to do if it still doesn’t work?

- Directly refer to the document entry in README.
- Prioritize keeping the error request, response and request log before continuing the investigation.