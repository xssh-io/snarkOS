CREATE TABLE solution
(
    datetime          DATETIME64(3),
    submitter_address String,
    submitter_ip      String,
    solution_id       String,
    epoch_hash        String,
    address           String,
    counter           Int32,
    target            Int32
) ENGINE = MergeTree()
      ORDER BY tuple();

CREATE TABLE solution_attempt
(
    datetime          DATETIME64(3),
    submitter_address String,
    submitter_ip      String,
    solution_id       String,
    epoch_hash        String,
    address           String,
    counter           Int32,
    target            Int32
) ENGINE = MergeTree()
      ORDER BY tuple();
