---
title: Gmail Setup
description: Connect your Gmail account to mxr
---

## Prerequisites

You need a Google Cloud project with the Gmail API enabled.

## Steps

1. Go to [Google Cloud Console](https://console.cloud.google.com)
2. Create a new project (or select existing)
3. Enable the Gmail API
4. Create OAuth 2.0 credentials (Desktop application)
5. Run the setup command:

```bash
mxr accounts add gmail
```

6. Follow the prompts to enter your client ID and secret
7. Authorize in your browser
8. Start syncing:

```bash
mxr sync
```
