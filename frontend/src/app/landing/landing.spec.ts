import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { Landing } from './landing';

describe('Landing', () => {
  let component: Landing;
  let router: {
    navigate: ReturnType<typeof vi.fn>;
  };
});
