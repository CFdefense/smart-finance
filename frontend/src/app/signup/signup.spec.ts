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
  }

  it('should reject incomplete signup information', () => {
    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });

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

  it('should reject duplicate email addresses', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 409 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe('An account with this email already exists.');
    expect(component.isSubmitting()).toBe(false);
  });

  it('should display validation errors for status 400', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 400 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe(
      'Please check your information and password requirements.',
    );
  });

  it('should display validation errors for status 422', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 422 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe(
      'Please check your information and password requirements.',
    );
  });

  it('should display a generic error for server failures', () => {
    authService.signup.mockReturnValue(throwError(() => ({ status: 500 })));

    fillForm();
    component.onSignup();

    expect(component.errorMessage()).toBe('Registration failed. Please try again later.');
  });

  it('should prevent duplicate submissions', () => {
    fillForm();
    component.isSubmitting.set(true);

    component.onSignup();

    expect(authService.signup).not.toHaveBeenCalled();
  });

  it('should reject a missing last name', () => {
    fillForm();
    component.lastName = '';

    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });

  it('should reject a missing email', () => {
    fillForm();
    component.email = '   ';

    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });

  it('should reject a missing password', () => {
    fillForm();
    component.password = '';

    component.onSignup();

    expect(component.errorMessage()).toBe('Please complete all fields.');
    expect(authService.signup).not.toHaveBeenCalled();
  });
});
