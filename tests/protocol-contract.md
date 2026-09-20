# Runtime Protocol Contract

The desktop host intentionally depends only on the SDK JSON-RPC surface.

Required requests:
- initialize
- session/prompt
- shutdown

Required notifications:
- session.event
- session.status
- subagent.started
- subagent.finished

The implementation correlates request ids to JSON-RPC responses. Notifications are forwarded to the frontend as runtime:event.

This file is a release checklist, not a second implementation of the Harness protocol.
