import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { CreateGoalRequest, Goal, GoalsService, UpdateGoalRequest } from './goals.service';

describe('GoalsService', () => {
  let service: GoalsService;
  let httpMock: HttpTestingController;

  const baseUrl = 'http://localhost:3001/api/goals';

  // A sample response matching the Rust Goals API.
  const mockGoal: Goal = {
    id: 1,
    name: 'Emergency Fund',
    description: 'Save for unexpected expenses',
    target_amount: 5000,
    target_date: null,
    account_id: 'account-123',
    baseline_balance: 1000,
    current_balance: 2000,
    progress_percent: 25,
    remaining: 3000,
    on_track: null,
    status: 'ACTIVE',
    created_at: '2026-11-04T12:00:00Z',
  };

  beforeEach(() => {
    // Configure Angular with our service and a fake HTTP backend.
    TestBed.configureTestingModule({
      providers: [GoalsService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(GoalsService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    // Verify that every HTTP request was handled by our tests.
    httpMock.verify();
  });

  /**
   * Verifies that getGoals retrieves all goals.
   * Checks the GET request and returns a simulated goals response.
   */
  it('should retrieve all goals', () => {
    let receivedGoals: Goal[] = [];

    service.getGoals().subscribe((response) => {
      receivedGoals = response.goals;
    });

    const req = httpMock.expectOne(baseUrl);

    expect(req.request.method).toBe('GET');
    expect(req.request.withCredentials).toBe(true);
    expect(req.request.params.has('status')).toBe(false);

    // Pretend Rust returned our example goal.
    req.flush({ goals: [mockGoal] });

    expect(receivedGoals).toEqual([mockGoal]);
  });

  /**
   * Verifies that goals can be filtered by their status.
   * Checks that the service includes the correct URL query parameter.
   */
  it('should filter goals by status', () => {
    service.getGoals('COMPLETED').subscribe();

    const req = httpMock.expectOne(`${baseUrl}?status=COMPLETED`);

    expect(req.request.method).toBe('GET');
    expect(req.request.params.get('status')).toBe('COMPLETED');
    expect(req.request.withCredentials).toBe(true);

    req.flush({ goals: [] });
  });

  /**
   * Verifies that getGoal retrieves one goal by its ID.
   * Checks the request URL and the returned goal data.
   */
  it('should retrieve a goal by ID', () => {
    let receivedGoal: Goal | undefined;

    service.getGoal(1).subscribe((goal) => {
      receivedGoal = goal;
    });

    const req = httpMock.expectOne(`${baseUrl}/1`);

    expect(req.request.method).toBe('GET');
    expect(req.request.withCredentials).toBe(true);

    req.flush(mockGoal);

    expect(receivedGoal).toEqual(mockGoal);
  });

  /**
   * Verifies that createGoal submits the required goal information.
   * Checks the POST request body and authentication credentials.
   */
  it('should create a goal', () => {
    const newGoal: CreateGoalRequest = {
      name: 'Emergency Fund',
      description: 'Save for unexpected expenses',
      target_amount: 5000,
      account_id: 'account-123',
    };

    service.createGoal(newGoal).subscribe();

    const req = httpMock.expectOne(baseUrl);

    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual(newGoal);
    expect(req.request.withCredentials).toBe(true);

    // Rust responds with HTTP 201 and no response body.
    req.flush(null, {
      status: 201,
      statusText: 'Created',
    });
  });

  /**
   * Verifies that updateGoal sends changes to an existing goal.
   * Checks the PUT request and its updated response.
   */
  it('should update an existing goal', () => {
    const changes: UpdateGoalRequest = {
      name: 'Updated Emergency Fund',
      target_amount: 6000,
    };

    let updatedGoal: Goal | undefined;

    service.updateGoal(1, changes).subscribe((goal) => {
      updatedGoal = goal;
    });

    const req = httpMock.expectOne(`${baseUrl}/1`);

    expect(req.request.method).toBe('PUT');
    expect(req.request.body).toEqual(changes);
    expect(req.request.withCredentials).toBe(true);

    const response: Goal = {
      ...mockGoal,
      name: 'Updated Emergency Fund',
      target_amount: 6000,
      remaining: 4000,
      progress_percent: 20,
    };

    req.flush(response);

    expect(updatedGoal).toEqual(response);
  });

  /**
   * Verifies that goals can be archived without deleting them.
   * Checks that the PUT request includes archived: true.
   */
  it('should archive a goal', () => {
    service.updateGoal(1, { archived: true }).subscribe();

    const req = httpMock.expectOne(`${baseUrl}/1`);

    expect(req.request.method).toBe('PUT');
    expect(req.request.body).toEqual({
      archived: true,
    });
    expect(req.request.withCredentials).toBe(true);

    req.flush({
      ...mockGoal,
      status: 'ARCHIVED',
    });
  });

  /**
   * Verifies that deleteGoal removes a goal by its ID.
   * Checks the authenticated DELETE request and handles HTTP 204.
   */
  it('should delete a goal', () => {
    let completed = false;

    service.deleteGoal(1).subscribe(() => {
      completed = true;
    });

    const req = httpMock.expectOne(`${baseUrl}/1`);

    expect(req.request.method).toBe('DELETE');
    expect(req.request.withCredentials).toBe(true);

    // Rust returns 204 No Content after successful deletion.
    req.flush(null, {
      status: 204,
      statusText: 'No Content',
    });

    expect(completed).toBe(true);
  });
});
