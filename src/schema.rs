table! {
    feeds (id) {
        id -> Integer,
        url -> Text,
        last_fetched_at -> Timestamp,
    }
}

table! {
    items (id) {
        id -> Integer,
        guid -> Text,
        link -> Text,
        title -> Nullable<Text>,
        fetched_at -> Timestamp,
        feed -> Integer,
    }
}

table! {
    notifications (id) {
        id -> Integer,
        item -> Integer,
        webhook -> Integer,
        sent -> Integer,
        sent_at -> Nullable<Timestamp>,
    }
}

table! {
    webhooks (id) {
        id -> Integer,
        url -> Text,
    }
}

joinable!(items -> feeds (feed));
joinable!(notifications -> items (item));
joinable!(notifications -> webhooks (webhook));

allow_tables_to_appear_in_same_query!(feeds, items, notifications, webhooks,);
