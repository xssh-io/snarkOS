-- CREATE TABLE solution (
--     id INT PRIMARY KEY AUTO_INCREMENT,
--     submitter_address VARCHAR(255) NOT NULL,
--     submitter_ip VARCHAR(255) NOT NULL,
--     solution_id INT NOT NULL,
--     epoch_hash INT NOT NULL,
--     address VARCHAR(255) NOT NULL,
--     counter INT NOT NULL,
--     target INT NOT NULL,
-- );

-- clickhouse dialect

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
