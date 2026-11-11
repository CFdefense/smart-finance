import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';

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
 * Response from GET /api/plaid/net-worth/history.
 *
 * Rust returns historical snapshots oldest to newest.
 */
export interface NetWorthHistoryResponse {
  snapshots: NetWorthSnapshot[];
}

/**
 * Handles communication between Angular and our Rust Plaid endpoints.
 *
 * This service only makes HTTP requests.
 * Our Angular components decide when to call these methods and
 * how to display the results.
 */
@Injectable({
  providedIn: 'root',
})
export class PlaidService {
  // Angular provides the HttpClient through dependency injection.
  private readonly http = inject(HttpClient);

  // All Plaid-related backend endpoints share this prefix.
  private readonly baseUrl = 'http://localhost:3001/api/plaid';

  /**
   * Retrieve the authenticated user's connected financial accounts.
   *
   * withCredentials sends the authentication cookie to the backend.
   */
  getAccounts(): Observable<PlaidAccountsResponse> {
    return this.http.get<PlaidAccountsResponse>(`${this.baseUrl}/accounts`, {
      withCredentials: true,
    });
  }

  /**
   * Retrieve historical linked-account net-worth snapshots.
   *
   * Rust determines which records belong to the signed-in user.
   * withCredentials sends the authentication cookie.
   */
  getNetWorthHistory(): Observable<NetWorthHistoryResponse> {
    return this.http.get<NetWorthHistoryResponse>(`${this.baseUrl}/net-worth/history`, {
      withCredentials: true,
    });
  }

  /**
   * Retrieve transactions for one account, or all accounts.
   *
   * The backend validates ownership, so the caller cannot
   * retrieve another user's transactions by guessing an ID.
   *
   * Defaults:
   * - 20 transactions per page.
   * - First page (offset 0).
   *
   * We let HttpParams handle query-string encoding.
   */
  getTransactions(
    accountId?: string,
    limit = 20,
    offset = 0,
  ): Observable<PlaidTransactionsResponse> {
    let params = new HttpParams().set('limit', limit).set('offset', offset);

    // An omitted accountId requests transactions from all accounts.
    if (accountId !== undefined) {
      params = params.set('account_id', accountId);
    }

    return this.http.get<PlaidTransactionsResponse>(`${this.baseUrl}/transactions`, {
      params,
      withCredentials: true,
    });
  }

  /**
   * Ask our backend for a short-lived Plaid Link token.
   *
   * The frontend will use this token to open Plaid's bank
   * connection interface.
   */
  createLinkToken(): Observable<PlaidLinkTokenResponse> {
    return this.http.post<PlaidLinkTokenResponse>(
      `${this.baseUrl}/link-token`,
      {},
      { withCredentials: true },
    );
  }

  /**
   * Exchange the public token received from Plaid Link.
   *
   * The backend stores the resulting access token securely.
   * Angular never receives or stores that access token.
   */
  exchangeToken(publicToken: string): Observable<PlaidExchangeTokenResponse> {
    return this.http.post<PlaidExchangeTokenResponse>(
      `${this.baseUrl}/exchange-token`,
      { public_token: publicToken },
      { withCredentials: true },
    );
  }

  /**
   * Disconnect an entire Plaid Item (bank connection).
   *
   * The Rust controller verifies ownership, contacts Plaid,
   * and removes the connection from PostgreSQL.
   */
  removeItem(itemId: string): Observable<void> {
    return this.http.delete<void>(`${this.baseUrl}/items/${encodeURIComponent(itemId)}`, {
      withCredentials: true,
    });
  }
}
