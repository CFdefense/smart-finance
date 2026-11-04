import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';

import { BudgetService, type Budget, type CreateBudgetRequest } from './budget.service';

describe('BudgetService', () => {
  let service: BudgetService;
  let httpMock: HttpTestingController;

  const baseUrl = 'http://localhost:3001/api/budgets';

  /**
   * Example budget returned by the Rust backend.
   */
  const mockBudget: Budget = {
    id: 1,
    name: 'Groceries',
    category: 'GROCERIES',
    monthly_limit: 400,
    active: true,
    month: '2026-11',
    spent: 150,
    remaining: 250,
    percent_used: 37.5,
    over_budget: false,
  };

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [BudgetService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(BudgetService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  /**
   * Ensure no unexpected HTTP requests remain after each test.
   */
  afterEach(() => {
    httpMock.verify();
  });

  it('retrieves budgets for the current month', () => {
    let result: Budget[] = [];

    service.getBudgets().subscribe((response) => {
      result = response.budgets;
    });

    const request = httpMock.expectOne(baseUrl);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    request.flush({
      month: '2026-11',
      budgets: [mockBudget],
    });

    expect(result).toEqual([mockBudget]);
  });

  it('retrieves budgets for a specified month', () => {
    service.getBudgets('2026-10').subscribe();

    const request = httpMock.expectOne(`${baseUrl}?month=2026-10`);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    request.flush({
      month: '2026-10',
      budgets: [],
    });
  });

  it('retrieves one budget by ID', () => {
    let result: Budget | undefined;

    service.getBudget(1, '2026-11').subscribe((budget) => {
      result = budget;
    });

    const request = httpMock.expectOne(`${baseUrl}/1?month=2026-11`);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    request.flush(mockBudget);

    expect(result).toEqual(mockBudget);
  });

  it('retrieves one budget without a month parameter', () => {
    service.getBudget(1).subscribe();

    const request = httpMock.expectOne(`${baseUrl}/1`);

    expect(request.request.method).toBe('GET');
    expect(request.request.params.has('month')).toBe(false);
    expect(request.request.withCredentials).toBe(true);

    request.flush(mockBudget);
  });

  it('creates a monthly budget', () => {
    const newBudget: CreateBudgetRequest = {
      name: 'Groceries',
      category: 'GROCERIES',
      monthly_limit: 400,
    };

    let completed = false;

    service.createBudget(newBudget).subscribe({
      complete: () => {
        completed = true;
      },
    });

    const request = httpMock.expectOne(baseUrl);

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual(newBudget);
    expect(request.request.withCredentials).toBe(true);

    request.flush(null, {
      status: 201,
      statusText: 'Created',
    });

    expect(completed).toBe(true);
  });

  it('updates an existing budget', () => {
    const changes = {
      name: 'Weekly groceries',
      monthly_limit: 500,
      active: false,
    };

    service.updateBudget(1, changes).subscribe((budget) => {
      expect(budget.monthly_limit).toBe(500);
      expect(budget.active).toBe(false);
    });

    const request = httpMock.expectOne(`${baseUrl}/1`);

    expect(request.request.method).toBe('PUT');
    expect(request.request.body).toEqual(changes);
    expect(request.request.withCredentials).toBe(true);

    request.flush({
      ...mockBudget,
      ...changes,
    });
  });

  it('deletes an existing budget', () => {
    let completed = false;

    service.deleteBudget(1).subscribe({
      complete: () => {
        completed = true;
      },
    });

    const request = httpMock.expectOne(`${baseUrl}/1`);

    expect(request.request.method).toBe('DELETE');
    expect(request.request.withCredentials).toBe(true);

    request.flush(null, {
      status: 204,
      statusText: 'No Content',
    });

    expect(completed).toBe(true);
  });
});
