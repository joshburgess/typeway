module Api exposing
    ( Article
    , ArticleResponse
    , ArticlesResponse
    , Comment
    , CommentResponse
    , CommentsResponse
    , Profile
    , ProfileResponse
    , TagsResponse
    , User
    , UserResponse
    , addComment
    , createArticle
    , getArticle
    , getArticles
    , getComments
    , getProfile
    , getTags
    , login
    , register
    )

import Http
import Json.Decode as D
import Json.Encode as E



-- TYPES


type alias User =
    { email : String
    , token : String
    , username : String
    , bio : Maybe String
    , image : Maybe String
    }


type alias Profile =
    { username : String
    , bio : Maybe String
    , image : Maybe String
    , following : Bool
    }


type alias Article =
    { slug : String
    , title : String
    , description : String
    , body : String
    , tagList : List String
    , createdAt : String
    , updatedAt : String
    , favorited : Bool
    , favoritesCount : Int
    , author : Profile
    }


type alias Comment =
    { id : Int
    , body : String
    , createdAt : String
    , updatedAt : String
    , author : Profile
    }


type alias UserResponse =
    { user : User }


type alias ProfileResponse =
    { profile : Profile }


type alias ArticleResponse =
    { article : Article }


type alias ArticlesResponse =
    { articles : List Article
    , articlesCount : Int
    }


type alias CommentResponse =
    { comment : Comment }


type alias CommentsResponse =
    { comments : List Comment }


type alias TagsResponse =
    { tags : List String }



-- DECODERS


userDecoder : D.Decoder User
userDecoder =
    D.map5 User
        (D.field "email" D.string)
        (D.field "token" D.string)
        (D.field "username" D.string)
        (D.maybe (D.field "bio" D.string))
        (D.maybe (D.field "image" D.string))


profileDecoder : D.Decoder Profile
profileDecoder =
    D.map4 Profile
        (D.field "username" D.string)
        (D.maybe (D.field "bio" D.string))
        (D.maybe (D.field "image" D.string))
        (D.field "following" D.bool)


articleDecoder : D.Decoder Article
articleDecoder =
    D.succeed Article
        |> andMap (D.field "slug" D.string)
        |> andMap (D.field "title" D.string)
        |> andMap (D.field "description" D.string)
        |> andMap (D.field "body" D.string)
        |> andMap (D.field "tagList" (D.list D.string))
        |> andMap (D.field "createdAt" D.string)
        |> andMap (D.field "updatedAt" D.string)
        |> andMap (D.field "favorited" D.bool)
        |> andMap (D.field "favoritesCount" D.int)
        |> andMap (D.field "author" profileDecoder)


commentDecoder : D.Decoder Comment
commentDecoder =
    D.map5 Comment
        (D.field "id" D.int)
        (D.field "body" D.string)
        (D.field "createdAt" D.string)
        (D.field "updatedAt" D.string)
        (D.field "author" profileDecoder)


andMap : D.Decoder a -> D.Decoder (a -> b) -> D.Decoder b
andMap =
    D.map2 (|>)



-- API CALLS


authHeader : Maybe String -> List Http.Header
authHeader maybeToken =
    case maybeToken of
        Just token ->
            [ Http.header "Authorization" ("Token " ++ token) ]

        Nothing ->
            []


login : String -> String -> String -> (Result Http.Error UserResponse -> msg) -> Cmd msg
login apiUrl email password toMsg =
    Http.request
        { method = "POST"
        , headers = []
        , url = apiUrl ++ "/api/users/login"
        , body =
            Http.jsonBody
                (E.object
                    [ ( "user"
                      , E.object
                            [ ( "email", E.string email )
                            , ( "password", E.string password )
                            ]
                      )
                    ]
                )
        , expect = Http.expectJson toMsg (D.map UserResponse (D.field "user" userDecoder))
        , timeout = Nothing
        , tracker = Nothing
        }


register : String -> String -> String -> String -> (Result Http.Error UserResponse -> msg) -> Cmd msg
register apiUrl username email password toMsg =
    Http.request
        { method = "POST"
        , headers = []
        , url = apiUrl ++ "/api/users"
        , body =
            Http.jsonBody
                (E.object
                    [ ( "user"
                      , E.object
                            [ ( "username", E.string username )
                            , ( "email", E.string email )
                            , ( "password", E.string password )
                            ]
                      )
                    ]
                )
        , expect = Http.expectJson toMsg (D.map UserResponse (D.field "user" userDecoder))
        , timeout = Nothing
        , tracker = Nothing
        }


getArticles : String -> Maybe String -> (Result Http.Error ArticlesResponse -> msg) -> Cmd msg
getArticles apiUrl token toMsg =
    Http.request
        { method = "GET"
        , headers = authHeader token
        , url = apiUrl ++ "/api/articles"
        , body = Http.emptyBody
        , expect =
            Http.expectJson toMsg
                (D.map2 ArticlesResponse
                    (D.field "articles" (D.list articleDecoder))
                    (D.field "articlesCount" D.int)
                )
        , timeout = Nothing
        , tracker = Nothing
        }


getArticle : String -> Maybe String -> String -> (Result Http.Error ArticleResponse -> msg) -> Cmd msg
getArticle apiUrl token slug toMsg =
    Http.request
        { method = "GET"
        , headers = authHeader token
        , url = apiUrl ++ "/api/articles/" ++ slug
        , body = Http.emptyBody
        , expect = Http.expectJson toMsg (D.map ArticleResponse (D.field "article" articleDecoder))
        , timeout = Nothing
        , tracker = Nothing
        }


createArticle : String -> String -> String -> String -> String -> List String -> (Result Http.Error ArticleResponse -> msg) -> Cmd msg
createArticle apiUrl token title description body tagList toMsg =
    Http.request
        { method = "POST"
        , headers = authHeader (Just token)
        , url = apiUrl ++ "/api/articles"
        , body =
            Http.jsonBody
                (E.object
                    [ ( "article"
                      , E.object
                            [ ( "title", E.string title )
                            , ( "description", E.string description )
                            , ( "body", E.string body )
                            , ( "tagList", E.list E.string tagList )
                            ]
                      )
                    ]
                )
        , expect = Http.expectJson toMsg (D.map ArticleResponse (D.field "article" articleDecoder))
        , timeout = Nothing
        , tracker = Nothing
        }


getComments : String -> Maybe String -> String -> (Result Http.Error CommentsResponse -> msg) -> Cmd msg
getComments apiUrl token slug toMsg =
    Http.request
        { method = "GET"
        , headers = authHeader token
        , url = apiUrl ++ "/api/articles/" ++ slug ++ "/comments"
        , body = Http.emptyBody
        , expect = Http.expectJson toMsg (D.map CommentsResponse (D.field "comments" (D.list commentDecoder)))
        , timeout = Nothing
        , tracker = Nothing
        }


addComment : String -> String -> String -> String -> (Result Http.Error CommentResponse -> msg) -> Cmd msg
addComment apiUrl token slug body toMsg =
    Http.request
        { method = "POST"
        , headers = authHeader (Just token)
        , url = apiUrl ++ "/api/articles/" ++ slug ++ "/comments"
        , body =
            Http.jsonBody
                (E.object
                    [ ( "comment"
                      , E.object [ ( "body", E.string body ) ]
                      )
                    ]
                )
        , expect = Http.expectJson toMsg (D.map CommentResponse (D.field "comment" commentDecoder))
        , timeout = Nothing
        , tracker = Nothing
        }


getTags : String -> (Result Http.Error TagsResponse -> msg) -> Cmd msg
getTags apiUrl toMsg =
    Http.get
        { url = apiUrl ++ "/api/tags"
        , expect = Http.expectJson toMsg (D.map TagsResponse (D.field "tags" (D.list D.string)))
        }


getProfile : String -> Maybe String -> String -> (Result Http.Error ProfileResponse -> msg) -> Cmd msg
getProfile apiUrl token username toMsg =
    Http.request
        { method = "GET"
        , headers = authHeader token
        , url = apiUrl ++ "/api/profiles/" ++ username
        , body = Http.emptyBody
        , expect = Http.expectJson toMsg (D.map ProfileResponse (D.field "profile" profileDecoder))
        , timeout = Nothing
        , tracker = Nothing
        }
