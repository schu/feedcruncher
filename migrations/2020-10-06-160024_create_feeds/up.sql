CREATE TABLE feeds (
	id INTEGER PRIMARY KEY NOT NULL,
	url TEXT NOT NULL UNIQUE,
	last_fetched_at TIMESTAMP NOT NULL
);
