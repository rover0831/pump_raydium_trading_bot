# Axum User Authentication API

A complete user authentication system built with Axum, MongoDB, and JWT tokens.

## Features

- User registration with email and username validation
- Secure password hashing using bcrypt
- JWT-based authentication
- MongoDB integration with proper indexing
- Input validation and error handling
- CORS support
- Comprehensive logging

## Prerequisites

- Rust (latest stable version)
- MongoDB running locally or accessible via connection string
- Docker (optional, for running MongoDB)

## Setup

### 1. Clone and Install Dependencies

```bash
cargo build
```

### 2. Environment Configuration

Copy the example environment file and configure your settings:

```bash
cp env.example .env
```

Edit `.env` with your configuration:

```env
MONGODB_URI=mongodb://localhost:27017
JWT_SECRET=your-super-secret-jwt-key-change-this-in-production
RUST_LOG=info
```

### 3. MongoDB Setup

#### Option A: Local MongoDB
```bash
# Install MongoDB locally
sudo apt-get install mongodb

# Start MongoDB service
sudo systemctl start mongodb
sudo systemctl enable mongodb
```

#### Option B: Docker MongoDB
```bash
docker run -d --name mongodb \
  -p 27017:27017 \
  -e MONGO_INITDB_ROOT_USERNAME=admin \
  -e MONGO_INITDB_ROOT_PASSWORD=password \
  mongo:latest
```

### 4. Run the Application

```bash
cargo run
```

The server will start on `http://localhost:3000`

## API Endpoints

### Health Check
```
GET /
```
Returns server status and health information.

### User Registration
```
POST /auth/signup
Content-Type: application/json

{
  "email": "user@example.com",
  "username": "username",
  "password": "password123"
}
```

**Response:**
```json
{
  "token": "jwt_token_here",
  "user": {
    "id": "user_id_here",
    "email": "user@example.com",
    "username": "username",
    "created_at": "2024-01-01T00:00:00Z"
  }
}
```

### User Sign In
```
POST /auth/signin
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "password123"
}
```

**Response:** Same as signup response.

### Get Current User
```
GET /auth/me
Authorization: Bearer <jwt_token>
```

**Response:**
```json
{
  "id": "user_id_here",
  "email": "user@example.com",
  "username": "username",
  "created_at": "2024-01-01T00:00:00Z"
}
```

## Error Responses

All endpoints return consistent error responses:

```json
{
  "error": "Error Type",
  "message": "Detailed error message"
}
```

Common HTTP status codes:
- `200` - Success
- `400` - Bad Request (validation errors)
- `401` - Unauthorized (invalid/missing token)
- `409` - Conflict (user already exists)
- `500` - Internal Server Error

## Security Features

- **Password Hashing**: Uses bcrypt with cost factor 12
- **JWT Tokens**: Secure token-based authentication
- **Input Validation**: Comprehensive request validation
- **Database Indexing**: Unique constraints on email and username
- **CORS**: Configurable cross-origin resource sharing

## Project Structure

```
src/
├── main.rs          # Application entry point
├── models.rs        # Data structures and validation
├── db.rs           # Database operations and MongoDB integration
├── auth.rs         # Authentication utilities (JWT, password hashing)
└── routes.rs       # HTTP route handlers
```

## Testing the API

### Using curl

**Signup:**
```bash
curl -X POST http://localhost:3000/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "username": "testuser",
    "password": "password123"
  }'
```

**Signin:**
```bash
curl -X POST http://localhost:3000/auth/signin \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "password": "password123"
  }'
```

**Get Current User:**
```bash
curl -X GET http://localhost:3000/auth/me \
  -H "Authorization: Bearer YOUR_JWT_TOKEN"
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MONGODB_URI` | MongoDB connection string | `mongodb://localhost:27017` |
| `JWT_SECRET` | Secret key for JWT signing | `your-secret-key-change-in-production` |
| `RUST_LOG` | Logging level | `info` |

## Production Considerations

1. **Change JWT Secret**: Use a strong, random secret key
2. **Environment Variables**: Store secrets in secure environment variables
3. **HTTPS**: Use HTTPS in production
4. **Rate Limiting**: Implement rate limiting for auth endpoints
5. **Monitoring**: Add application monitoring and logging
6. **Database**: Use MongoDB Atlas or managed MongoDB service

## Troubleshooting

### Common Issues

1. **MongoDB Connection Failed**
   - Ensure MongoDB is running
   - Check connection string in `.env`
   - Verify network access

2. **JWT Token Invalid**
   - Check `JWT_SECRET` in environment
   - Ensure token format: `Bearer <token>`
   - Verify token hasn't expired

3. **Validation Errors**
   - Email must be valid format
   - Username: 3-30 characters
   - Password: minimum 8 characters

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is open source and available under the MIT License.
