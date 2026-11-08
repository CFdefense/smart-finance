import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';

/** A goal's status, computed by the Rust backend from the linked account balance. */
export type GoalStatus = 'ACTIVE' | 'COMPLETED' | 'ARCHIVED';

/** A saved goal, including the current balance and progress from the backend. */
export interface Goal {
  id: number;
  name: string;
  description: string | null;
  target_amount: number;
  target_date: string | null;
  account_id: string;
  baseline_balance: number;
  current_balance: number;
  progress_percent: number;
  remaining: number;
  on_track: boolean | null;
  status: GoalStatus;
  created_at: string;
}

/** The collection returned by GET /api/goals. */
export interface GoalsResponse {
  goals: Goal[];
}

/** Data required to create a goal. Plaid account ownership is checked by Rust. */
export interface CreateGoalRequest {
  name: string;
  target_amount: number;
  account_id: string;
  description?: string | null;
  target_date?: string | null;
  baseline_balance?: number;
}

/**
 * Fields that can be changed on an existing goal.
 * A null description or date clears that field; undefined leaves it unchanged.
 * If baseline_balance is supplied, account_id must also be supplied.
 */
export interface UpdateGoalRequest {
  name?: string;
  description?: string | null;
  target_amount?: number;
  target_date?: string | null;
  account_id?: string;
  baseline_balance?: number;
  archived?: boolean;
}

/**
 * Connects Angular Goals components to the existing authenticated Rust API.
 * The backend calculates goal progress and validates ownership, amounts, and dates.
 */
@Injectable({ providedIn: 'root' })
export class GoalsService {
  private readonly http = inject(HttpClient);
  private readonly baseUrl = 'http://localhost:3001/api/goals';
  private readonly options = { withCredentials: true };

  /** Fetch goals, optionally filtered by their computed status. */
  getGoals(status?: GoalStatus): Observable<GoalsResponse> {
    const params = status ? new HttpParams().set('status', status) : new HttpParams();
    return this.http.get<GoalsResponse>(this.baseUrl, {
      ...this.options,
      params,
    });
  }

  /** Fetch one goal with its latest balance-derived progress. */
  getGoal(id: number): Observable<Goal> {
    return this.http.get<Goal>(`${this.baseUrl}/${id}`, this.options);
  }

  /** Create a goal. The API returns HTTP 201 without a response body. */
  createGoal(request: CreateGoalRequest): Observable<void> {
    return this.http.post<void>(this.baseUrl, request, this.options);
  }

  /** Update provided fields, including archiving or unarchiving a goal. */
  updateGoal(id: number, changes: UpdateGoalRequest): Observable<Goal> {
    return this.http.put<Goal>(`${this.baseUrl}/${id}`, changes, this.options);
  }

  /** Permanently delete a goal. The API returns HTTP 204 without a body. */
  deleteGoal(id: number): Observable<void> {
    return this.http.delete<void>(`${this.baseUrl}/${id}`, this.options);
  }
}
