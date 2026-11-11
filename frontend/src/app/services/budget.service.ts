import { inject, Injectable } from '@angular/core';
import { HttpClient, HttpParams } from '@angular/common/http';
import { Observable } from 'rxjs';

/**
 * Import shared budget data models.
 *
 * The service handles API requests while the models
 * describe the data sent and received.
 */
import type {
  Budget,
  BudgetsResponse,
  CategorySpendingResponse,
  CreateBudgetRequest,
  UpdateBudgetRequest,
} from '../models/budget.models';

/**
 * Preserve existing imports elsewhere in the frontend.
 *
 * Components and tests that currently import these
 * types from budget.service.ts will continue working.
 */
export type {
  BudgetCategory,
  CreateBudgetRequest,
  UpdateBudgetRequest,
  Budget,
  BudgetsResponse,
  CategorySpending,
  CategorySpendingResponse,
} from '../models/budget.models';

/**
 * Communicates with the Rust budget endpoints.
 *
 * All requests include the user's authentication cookie.
 */
@Injectable({
  providedIn: 'root',
})
export class BudgetService {
  private readonly http = inject(HttpClient);

  private readonly baseUrl = 'http://localhost:3001/api/budgets';

  /**
   * Returns the user's budgets and their spending.
   *
   * An optional month can be supplied in YYYY-MM format.
   * Without one, the backend uses the current month.
   */
  getBudgets(month?: string): Observable<BudgetsResponse> {
    const params = month ? new HttpParams().set('month', month) : new HttpParams();

    return this.http.get<BudgetsResponse>(this.baseUrl, {
      params,
      withCredentials: true,
    });
  }

  /**
   * Fetch monthly spending grouped by category.
   *
   * This uses the dedicated Rust endpoint rather than
   * trying to reconstruct spending from the budgets list.
   *
   * When month is omitted, Rust uses the current UTC month.
   */
  getSpendingByCategory(month?: string): Observable<CategorySpendingResponse> {
    const params = month ? new HttpParams().set('month', month) : new HttpParams();

    return this.http.get<CategorySpendingResponse>(`${this.baseUrl}/spending-by-category`, {
      params,
      withCredentials: true,
    });
  }

  /**
   * Retrieves a single budget by its ID.
   */
  getBudget(id: number, month?: string): Observable<Budget> {
    const params = month ? new HttpParams().set('month', month) : new HttpParams();

    return this.http.get<Budget>(`${this.baseUrl}/${id}`, {
      params,
      withCredentials: true,
    });
  }

  /**
   * Creates a monthly spending budget.
   *
   * The backend returns HTTP 201 with no response body.
   */
  createBudget(request: CreateBudgetRequest): Observable<void> {
    return this.http.post<void>(this.baseUrl, request, {
      withCredentials: true,
    });
  }

  /**
   * Updates an existing budget's name, limit, or active state.
   */
  updateBudget(id: number, changes: UpdateBudgetRequest): Observable<Budget> {
    return this.http.put<Budget>(`${this.baseUrl}/${id}`, changes, { withCredentials: true });
  }

  /**
   * Permanently deletes a budget.
   *
   * The backend returns HTTP 204 with no response body.
   */
  deleteBudget(id: number): Observable<void> {
    return this.http.delete<void>(`${this.baseUrl}/${id}`, {
      withCredentials: true,
    });
  }
}
