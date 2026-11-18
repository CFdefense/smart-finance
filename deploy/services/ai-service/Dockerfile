# Dockerfile to build the AI service

#FROM base image -- in this case python
FROM python:3.12.6-slim

# Get uv
COPY --from=ghcr.io/astral-sh/uv:0.11.7 /uv /uvx /bin/

# Set working directory
WORKDIR /app

# Install dependencies
COPY pyproject.toml uv.lock ./
RUN uv sync --locked --no-dev --no-install-project

# Copy everything else into working dir. Separated for layering -- we don't need
# to reinstall dependences every time. 
COPY . .
RUN uv sync --locked --no-dev --no-install-project

ENV PATH=/app/.venv/bin

EXPOSE 8000

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]