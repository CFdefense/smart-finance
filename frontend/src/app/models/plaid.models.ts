/** Shared plaid API data models. */

/**
 * Represents a financial account returned by the Rust Plaid API.
 *
 * A single bank connection (item_id) may contain multiple accounts.
 * For example, one bank could provide both checking and savings.
 */
export interface PlaidAccount {
  account_id: string;
  item_id: string;
  // Actual financial institution name, such as Chase.
  institution_name: string | null;
  name: string;
  official_name: string | null;
  account_type: string;
  account_subtype: string | null;
  mask: string | null;
  current_balance: number | null;
  available_balance: number | null;
  currency_code: string | null;
}

/** Response from GET /api/plaid/accounts. */
export interface PlaidAccountsResponse {
  accounts: PlaidAccount[];
}

/**
 * A single transaction returned by our Rust API.
 *
 * Positive amounts represent money leaving the account.
 * Negative amounts generally represent incoming money or credits.
 */
export interface PlaidTransaction {
  transaction_id: string;
  account_id: string;
  name: string;
  amount: number;
  date: string;
  pending: boolean;
  category: string;
}

/**
 * A paginated collection of transactions.
 *
 * total: All transactions matching the account filter.
 * limit: Maximum number returned in this request.
 * offset: Number of records skipped.
 */
export interface PlaidTransactionsResponse {
  transactions: PlaidTransaction[];
  total: number | null;
  limit: number;
  offset: number;
}

/** Response containing the temporary token used to open Plaid Link. */
export interface PlaidLinkTokenResponse {
  link_token: string;
}

/** Response confirming that Plaid Link connected a bank successfully. */
export interface PlaidExchangeTokenResponse {
  success: boolean;
  item_id: string;
}

/**
 * A historical net-worth snapshot saved by Rust.
 *
 * Each snapshot represents one calendar day.
 * Amounts are in USD, matching the backend calculation.
 */
export interface NetWorthSnapshot {
  snapshot_date: string;
  amount: number;
  included_accounts: number;
  excluded_accounts: number;

  /**
   * Indicates whether this snapshot contains generated
   * sandbox demonstration data.
   *
   * Optional for backward compatibility with existing
   * test fixtures while we update them.
   */
  is_demo?: boolean;
}

/**
 * Response from GET /api/accounts/net-worth/history.
 *
 * Rust returns historical snapshots oldest to newest.
 */
export interface NetWorthHistoryResponse {
  snapshots: NetWorthSnapshot[];
}
