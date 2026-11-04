import os

import psycopg


def get_connection() -> psycopg.Connection:
    """Open a connection to the app's Postgres database."""
    return psycopg.connect(os.environ["DATABASE_URL"])
