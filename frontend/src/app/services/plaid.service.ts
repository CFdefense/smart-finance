import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { API_BASE_URL } from '../api-config';

import type {
  PlaidAccountsResponse,
  PlaidTransactionsResponse,
  PlaidLinkTokenResponse,
  PlaidExchangeTokenResponse,
  NetWorthHistoryResponse,
} from '../models/plaid.models';

export type {
  PlaidAccount,
  PlaidAccountsResponse,
  PlaidTransaction,
  PlaidTransactionsResponse,
  PlaidLinkTokenResponse,
  PlaidExchangeTokenResponse,
  NetWorthSnapshot,
  NetWorthHistoryResponse,
} from '../models/plaid.models';

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
  private readonly baseUrl = `${API_BASE_URL}/plaid`;

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
    return this.http.get<NetWorthHistoryResponse>(
      'http://localhost:3001/api/accounts/net-worth/history',
      {
        withCredentials: true,
      },
    );
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
