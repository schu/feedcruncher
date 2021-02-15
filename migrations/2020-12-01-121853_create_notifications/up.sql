CREATE TABLE IF NOT EXISTS notifications (
	id INTEGER PRIMARY KEY NOT NULL,
	item INTEGER NOT NULL,
	webhook INTEGER NOT NULL,
	sent INTEGER NOT NULL,
	sent_at TIMESTAMP,
	FOREIGN KEY(item) REFERENCES items(id),
	FOREIGN KEY(webhook) REFERENCES webhooks(id)
)
