from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()

class ChatRequest(BaseModel):
    user_id: int
    message: str

class ChatResponse(BaseModel):
    response: str

def generate_answer(user_id: int, message: str) -> str:
    '''Placeholder until we connect an LLM service'''
    return f'You ({user_id}) asked {message}'

@app.post('/chat')
def chat(request: ChatRequest) -> ChatResponse:
    answer = generate_answer(request.user_id, request.message)
    return ChatResponse(response=answer)

@app.get('/health')
def health():
    return {'status': 'ok'}

