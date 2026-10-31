FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 HF_HOME=/cache/huggingface
WORKDIR /app
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt \
    && groupadd --gid 10001 finbert \
    && useradd --uid 10001 --gid 10001 --no-create-home finbert \
    && mkdir -p /cache/huggingface \
    && chown -R 10001:10001 /cache
COPY serve.py ./
USER 10001:10001
EXPOSE 8000
CMD ["python", "serve.py"]
