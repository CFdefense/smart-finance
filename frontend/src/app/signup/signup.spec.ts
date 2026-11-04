import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';

import { Signup } from './signup';
import { AuthService } from '../services/auth.service';

describe('Signup', () => {
  let component: Signup;
  let authService: {
    signup: ReturnType<typeof vi.fn>;
  };
  let router: {
    navigate: ReturnType<typeof vi.fn>;
  };

  beforeEach(async () => {
    authService = {
      signup: vi.fn().mockReturnValue(of(void 0)),
    };

    router = {
      navigate: vi.fn(),
    };

    await TestBed.configureTestingModule({
      imports: [Signup],
      providers: [
        { provide: AuthService, useValue: authService },
        { provide: Router, useValue: router },
      ],
    })
      .overrideComponent(Signup, {
        set: { template: '' },
      })
      .compileComponents();

    const fixture = TestBed.createComponent(Signup);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  function fillForm(): void {
    component.firstName = 'John';
    component.lastName = 'Smith';
    component.email = 'john@example.com';
    component.password = 'Password123!';
    component.confirmPassword = 'Password123!';
  }

  /**
   * Checks that an entirely empty signup form is rejected.
   * Submits blank fields and verifies an error appears without calling signup.
   */
  it('should reject incomplete signup information', () => {
    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });

  /**
   * Checks successful signup, whitespace trimming, and dashboard navigation.
   * Submits valid data with padded fields and inspects the service call and route.
   */
  it('should register successfully and navigate to dashboard', () => {
    fillForm();
    component.firstName = '  John  ';
    component.lastName = '  Smith  ';
    component.email = '  john@example.com  ';

    component.onSignup();

    expect(authService.signup).toHaveBeenCalledWith(
      'John',
      'Smith',
      'john@example.com',
      'Password123!',
    );
    expect(router.navigate).toHaveBeenCalledWith(['/dashboard']);
    expect(component.isSubmitting()).toBe(false);
  });

  /**
   * Checks that signup displays a useful message for an existing email.
   * Mocks a 409 response and verifies the message and submission state.
   */
  it('should reject duplicate email addresses', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 409 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe('An account with this email already exists.');
    expect(component.isSubmitting()).toBe(false);
  });

  /**
   * Checks the signup message for a bad-request validation failure.
   * Mocks a 400 response and verifies the expected validation text.
   */
  it('should display validation errors for status 400', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 400 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe(
      'Please check your information and password requirements.',
    );
  });

  /**
   * Checks the signup message for an unprocessable-input failure.
   * Mocks a 422 response and verifies the expected validation text.
   */
  it('should display validation errors for status 422', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 422 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe(
      'Please check your information and password requirements.',
    );
  });

  /**
   * Checks that unexpected server errors get a general signup message.
   * Mocks a 500 response and verifies the failure text.
   */
  it('should display a generic error for server failures', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 500 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe('Registration failed. Please try again later.');
  });

  /**
   * Checks that an in-progress signup cannot be submitted again.
   * Sets the submitting state and verifies the signup service is not called.
   */
  it('should prevent duplicate submissions', () => {
    fillForm();
    component.isSubmitting.set(true);

    component.onSignup();

    expect(authService.signup).not.toHaveBeenCalled();
  });

  /**
   * Checks that a last name is required for signup.
   * Clears the last name and verifies validation blocks the service call.
   */
  it('should reject a missing last name', () => {
    fillForm();
    component.lastName = '';

    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });

  /**
   * Checks that whitespace-only email input is rejected.
   * Supplies a blank email and verifies validation blocks the service call.
   */
  it('should reject a missing email', () => {
    fillForm();
    component.email = '   ';

    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });

  /**
   * Checks that signup requires a password.
   * Clears the password and verifies validation blocks the service call.
   */
  it('should reject a missing password', () => {
    fillForm();
    component.password = '';

    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });
});
