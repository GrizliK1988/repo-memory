# Marketing review

Use the local review service to view files in `docs/` and `spec/` from a phone and
leave correction notes. Comments are recorded in `docs/.review-comments.json`, which
is intentionally kept out of the document list.

```bash
python3 tools/marketing_review_server.py --port 8010
```

From a device on the same Wi-Fi network, open `http://<computer-LAN-IP>:8010/`.
