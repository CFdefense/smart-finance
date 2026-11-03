import { HttpClient } from '@angular/common/http';
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
