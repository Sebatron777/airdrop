\COPY token_sale_contribution_codes(contribution_code) FROM './extracts/token_sale_contribution_codes3.csv' DELIMITER ',' CSV
\COPY gaia_addresses(address) FROM './extracts/gaia_addresses.csv' CSV
\COPY gaia_delegators(address, coins) FROM './extracts/gaia_addresses2.csv' CSV
