module Main exposing (main)

import Api
import Browser
import Browser.Navigation as Nav
import Html exposing (..)
import Html.Attributes exposing (..)
import Html.Events exposing (..)
import Http
import Json.Decode as D
import Json.Encode as E
import Url
import Url.Parser as Parser exposing ((</>))


main : Program Flags Model Msg
main =
    Browser.application
        { init = init
        , view = view
        , update = update
        , subscriptions = \_ -> Sub.none
        , onUrlChange = UrlChanged
        , onUrlRequest = LinkClicked
        }



-- FLAGS


type alias Flags =
    { apiUrl : String }



-- MODEL


type alias Model =
    { key : Nav.Key
    , page : Page
    , apiUrl : String
    , token : Maybe String
    , user : Maybe Api.User
    , errors : List String
    }


type Page
    = Home HomeModel
    | Login LoginModel
    | Register RegisterModel
    | ArticleDetail ArticleDetailModel
    | NewArticle NewArticleModel
    | Profile ProfileModel
    | NotFound


type alias HomeModel =
    { articles : List Api.Article
    , tags : List String
    , loading : Bool
    }


type alias LoginModel =
    { email : String
    , password : String
    , loading : Bool
    }


type alias RegisterModel =
    { username : String
    , email : String
    , password : String
    , loading : Bool
    }


type alias ArticleDetailModel =
    { article : Maybe Api.Article
    , comments : List Api.Comment
    , newComment : String
    , loading : Bool
    }


type alias NewArticleModel =
    { title : String
    , description : String
    , body : String
    , tags : String
    , loading : Bool
    }


type alias ProfileModel =
    { profile : Maybe Api.Profile
    , articles : List Api.Article
    , loading : Bool
    }


init : Flags -> Url.Url -> Nav.Key -> ( Model, Cmd Msg )
init flags url key =
    let
        model =
            { key = key
            , page = Home { articles = [], tags = [], loading = True }
            , apiUrl = flags.apiUrl
            , token = Nothing
            , user = Nothing
            , errors = []
            }
    in
    navigateTo url model



-- ROUTING


type Route
    = HomeRoute
    | LoginRoute
    | RegisterRoute
    | ArticleRoute String
    | NewArticleRoute
    | ProfileRoute String


routeParser : Parser.Parser (Route -> a) a
routeParser =
    Parser.oneOf
        [ Parser.map HomeRoute Parser.top
        , Parser.map LoginRoute (Parser.s "login")
        , Parser.map RegisterRoute (Parser.s "register")
        , Parser.map NewArticleRoute (Parser.s "editor")
        , Parser.map ArticleRoute (Parser.s "article" </> Parser.string)
        , Parser.map ProfileRoute (Parser.s "profile" </> Parser.string)
        ]


navigateTo : Url.Url -> Model -> ( Model, Cmd Msg )
navigateTo url model =
    case Parser.parse routeParser url of
        Just HomeRoute ->
            ( { model | page = Home { articles = [], tags = [], loading = True } }
            , Cmd.batch
                [ Api.getArticles model.apiUrl model.token GotArticles
                , Api.getTags model.apiUrl GotTags
                ]
            )

        Just LoginRoute ->
            ( { model | page = Login { email = "", password = "", loading = False }, errors = [] }
            , Cmd.none
            )

        Just RegisterRoute ->
            ( { model | page = Register { username = "", email = "", password = "", loading = False }, errors = [] }
            , Cmd.none
            )

        Just (ArticleRoute slug) ->
            ( { model | page = ArticleDetail { article = Nothing, comments = [], newComment = "", loading = True } }
            , Cmd.batch
                [ Api.getArticle model.apiUrl model.token slug GotArticle
                , Api.getComments model.apiUrl model.token slug GotComments
                ]
            )

        Just NewArticleRoute ->
            ( { model | page = NewArticle { title = "", description = "", body = "", tags = "", loading = False } }
            , Cmd.none
            )

        Just (ProfileRoute username) ->
            ( { model | page = Profile { profile = Nothing, articles = [], loading = True } }
            , Api.getProfile model.apiUrl model.token username GotProfile
            )

        Nothing ->
            ( { model | page = NotFound }, Cmd.none )



-- UPDATE


type Msg
    = LinkClicked Browser.UrlRequest
    | UrlChanged Url.Url
    | GotArticles (Result Http.Error Api.ArticlesResponse)
    | GotTags (Result Http.Error Api.TagsResponse)
    | GotArticle (Result Http.Error Api.ArticleResponse)
    | GotComments (Result Http.Error Api.CommentsResponse)
    | GotProfile (Result Http.Error Api.ProfileResponse)
      -- Login
    | SetLoginEmail String
    | SetLoginPassword String
    | SubmitLogin
    | GotLogin (Result Http.Error Api.UserResponse)
      -- Register
    | SetRegisterUsername String
    | SetRegisterEmail String
    | SetRegisterPassword String
    | SubmitRegister
    | GotRegister (Result Http.Error Api.UserResponse)
      -- New Article
    | SetArticleTitle String
    | SetArticleDescription String
    | SetArticleBody String
    | SetArticleTags String
    | SubmitArticle
    | GotNewArticle (Result Http.Error Api.ArticleResponse)
      -- Comments
    | SetNewComment String
    | SubmitComment String
    | GotNewComment (Result Http.Error Api.CommentResponse)
      -- Auth
    | Logout


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        LinkClicked (Browser.Internal url) ->
            ( model, Nav.pushUrl model.key (Url.toString url) )

        LinkClicked (Browser.External href) ->
            ( model, Nav.load href )

        UrlChanged url ->
            navigateTo url model

        GotArticles result ->
            case ( model.page, result ) of
                ( Home home, Ok resp ) ->
                    ( { model | page = Home { home | articles = resp.articles, loading = False } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        GotTags result ->
            case ( model.page, result ) of
                ( Home home, Ok resp ) ->
                    ( { model | page = Home { home | tags = resp.tags } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        GotArticle result ->
            case ( model.page, result ) of
                ( ArticleDetail detail, Ok resp ) ->
                    ( { model | page = ArticleDetail { detail | article = Just resp.article, loading = False } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        GotComments result ->
            case ( model.page, result ) of
                ( ArticleDetail detail, Ok resp ) ->
                    ( { model | page = ArticleDetail { detail | comments = resp.comments } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        GotProfile result ->
            case ( model.page, result ) of
                ( Profile prof, Ok resp ) ->
                    ( { model | page = Profile { prof | profile = Just resp.profile, loading = False } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        -- Login
        SetLoginEmail email ->
            case model.page of
                Login login ->
                    ( { model | page = Login { login | email = email } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SetLoginPassword pw ->
            case model.page of
                Login login ->
                    ( { model | page = Login { login | password = pw } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SubmitLogin ->
            case model.page of
                Login login ->
                    ( { model | page = Login { login | loading = True }, errors = [] }
                    , Api.login model.apiUrl login.email login.password GotLogin
                    )

                _ ->
                    ( model, Cmd.none )

        GotLogin result ->
            case result of
                Ok resp ->
                    ( { model
                        | token = Just resp.user.token
                        , user = Just resp.user
                        , errors = []
                      }
                    , Nav.pushUrl model.key "/"
                    )

                Err _ ->
                    ( { model | errors = [ "Invalid email or password" ] }
                    , Cmd.none
                    )

        -- Register
        SetRegisterUsername u ->
            case model.page of
                Register reg ->
                    ( { model | page = Register { reg | username = u } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SetRegisterEmail e ->
            case model.page of
                Register reg ->
                    ( { model | page = Register { reg | email = e } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SetRegisterPassword p ->
            case model.page of
                Register reg ->
                    ( { model | page = Register { reg | password = p } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SubmitRegister ->
            case model.page of
                Register reg ->
                    ( { model | page = Register { reg | loading = True }, errors = [] }
                    , Api.register model.apiUrl reg.username reg.email reg.password GotRegister
                    )

                _ ->
                    ( model, Cmd.none )

        GotRegister result ->
            case result of
                Ok resp ->
                    ( { model
                        | token = Just resp.user.token
                        , user = Just resp.user
                        , errors = []
                      }
                    , Nav.pushUrl model.key "/"
                    )

                Err _ ->
                    ( { model | errors = [ "Registration failed — username or email may be taken" ] }
                    , Cmd.none
                    )

        -- New Article
        SetArticleTitle t ->
            case model.page of
                NewArticle na ->
                    ( { model | page = NewArticle { na | title = t } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SetArticleDescription d ->
            case model.page of
                NewArticle na ->
                    ( { model | page = NewArticle { na | description = d } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SetArticleBody b ->
            case model.page of
                NewArticle na ->
                    ( { model | page = NewArticle { na | body = b } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SetArticleTags t ->
            case model.page of
                NewArticle na ->
                    ( { model | page = NewArticle { na | tags = t } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SubmitArticle ->
            case model.page of
                NewArticle na ->
                    case model.token of
                        Just token ->
                            let
                                tagList =
                                    na.tags
                                        |> String.split ","
                                        |> List.map String.trim
                                        |> List.filter (not << String.isEmpty)
                            in
                            ( { model | page = NewArticle { na | loading = True } }
                            , Api.createArticle model.apiUrl token na.title na.description na.body tagList GotNewArticle
                            )

                        Nothing ->
                            ( { model | errors = [ "You must be logged in" ] }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        GotNewArticle result ->
            case result of
                Ok resp ->
                    ( model, Nav.pushUrl model.key ("/article/" ++ resp.article.slug) )

                Err _ ->
                    ( { model | errors = [ "Failed to create article" ] }, Cmd.none )

        -- Comments
        SetNewComment c ->
            case model.page of
                ArticleDetail detail ->
                    ( { model | page = ArticleDetail { detail | newComment = c } }, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        SubmitComment slug ->
            case ( model.page, model.token ) of
                ( ArticleDetail detail, Just token ) ->
                    ( model
                    , Api.addComment model.apiUrl token slug detail.newComment GotNewComment
                    )

                _ ->
                    ( model, Cmd.none )

        GotNewComment result ->
            case ( model.page, result ) of
                ( ArticleDetail detail, Ok resp ) ->
                    ( { model
                        | page =
                            ArticleDetail
                                { detail
                                    | comments = resp.comment :: detail.comments
                                    , newComment = ""
                                }
                      }
                    , Cmd.none
                    )

                _ ->
                    ( model, Cmd.none )

        Logout ->
            ( { model | token = Nothing, user = Nothing }
            , Nav.pushUrl model.key "/"
            )



-- VIEW


view : Model -> Browser.Document Msg
view model =
    { title = "Conduit — Wayward"
    , body =
        [ viewNav model
        , div [ class "max-w-4xl mx-auto px-4 py-8" ]
            [ viewErrors model.errors
            , viewPage model
            ]
        , viewFooter
        ]
    }


viewNav : Model -> Html Msg
viewNav model =
    nav [ class "bg-white shadow-sm border-b" ]
        [ div [ class "max-w-4xl mx-auto px-4 py-3 flex items-center justify-between" ]
            [ a [ href "/", class "text-xl font-bold text-brand" ] [ text "conduit" ]
            , div [ class "flex gap-4 text-sm" ]
                (case model.user of
                    Just user ->
                        [ a [ href "/", class "text-gray-600 hover:text-gray-900" ] [ text "Home" ]
                        , a [ href "/editor", class "text-gray-600 hover:text-gray-900" ] [ text "New Article" ]
                        , a [ href ("/profile/" ++ user.username), class "text-gray-600 hover:text-gray-900" ] [ text user.username ]
                        , button [ onClick Logout, class "text-gray-600 hover:text-gray-900 cursor-pointer" ] [ text "Sign Out" ]
                        ]

                    Nothing ->
                        [ a [ href "/", class "text-gray-600 hover:text-gray-900" ] [ text "Home" ]
                        , a [ href "/login", class "text-gray-600 hover:text-gray-900" ] [ text "Sign In" ]
                        , a [ href "/register", class "text-gray-600 hover:text-gray-900" ] [ text "Sign Up" ]
                        ]
                )
            ]
        ]


viewErrors : List String -> Html Msg
viewErrors errors =
    if List.isEmpty errors then
        text ""

    else
        div [ class "bg-red-50 border border-red-200 rounded p-3 mb-4" ]
            (List.map (\e -> p [ class "text-red-700 text-sm" ] [ text e ]) errors)


viewFooter : Html Msg
viewFooter =
    footer [ class "mt-16 py-6 text-center text-xs text-gray-400 border-t" ]
        [ text "Powered by "
        , a [ href "https://github.com/joshburgess/wayward", class "text-brand" ] [ text "Wayward" ]
        , text " — a type-level web framework for Rust"
        ]


viewPage : Model -> Html Msg
viewPage model =
    case model.page of
        Home home ->
            viewHome home

        Login login ->
            viewLogin login

        Register reg ->
            viewRegister reg

        ArticleDetail detail ->
            viewArticleDetail model detail

        NewArticle na ->
            viewNewArticle na

        Profile prof ->
            viewProfile prof

        NotFound ->
            div [ class "text-center py-16" ]
                [ h1 [ class "text-2xl font-bold" ] [ text "Page not found" ] ]


viewHome : HomeModel -> Html Msg
viewHome home =
    div []
        [ div [ class "bg-brand text-white text-center py-10 -mx-4 mb-8 rounded-lg" ]
            [ h1 [ class "text-3xl font-bold" ] [ text "conduit" ]
            , p [ class "mt-2 text-green-100" ] [ text "A place to share your knowledge." ]
            ]
        , div [ class "flex gap-8" ]
            [ div [ class "flex-1" ]
                (if home.loading then
                    [ p [ class "text-gray-500" ] [ text "Loading articles..." ] ]

                 else if List.isEmpty home.articles then
                    [ p [ class "text-gray-500" ] [ text "No articles yet." ] ]

                 else
                    List.map viewArticlePreview home.articles
                )
            , div [ class "w-56" ]
                [ h3 [ class "text-sm font-semibold text-gray-500 mb-2" ] [ text "Popular Tags" ]
                , div [ class "flex flex-wrap gap-1" ]
                    (List.map
                        (\tag -> span [ class "bg-gray-200 text-gray-700 text-xs px-2 py-1 rounded-full" ] [ text tag ])
                        home.tags
                    )
                ]
            ]
        ]


viewArticlePreview : Api.Article -> Html Msg
viewArticlePreview article =
    div [ class "border-b py-4" ]
        [ div [ class "flex items-center gap-2 mb-2" ]
            [ a
                [ href ("/profile/" ++ article.author.username)
                , class "text-sm text-brand font-medium"
                ]
                [ text article.author.username ]
            , span [ class "text-xs text-gray-400" ] [ text article.createdAt ]
            ]
        , a [ href ("/article/" ++ article.slug) ]
            [ h2 [ class "text-lg font-semibold hover:text-brand" ] [ text article.title ]
            , p [ class "text-gray-500 text-sm mt-1" ] [ text article.description ]
            ]
        , div [ class "flex gap-1 mt-2" ]
            (List.map
                (\tag -> span [ class "text-xs text-gray-400 border rounded px-2 py-0.5" ] [ text tag ])
                article.tagList
            )
        ]


viewLogin : LoginModel -> Html Msg
viewLogin login =
    div [ class "max-w-md mx-auto" ]
        [ h1 [ class "text-2xl font-bold text-center mb-2" ] [ text "Sign In" ]
        , p [ class "text-center text-sm text-gray-500 mb-6" ]
            [ a [ href "/register", class "text-brand" ] [ text "Need an account?" ] ]
        , Html.form [ onSubmit SubmitLogin, class "space-y-4" ]
            [ input
                [ type_ "email"
                , placeholder "Email"
                , value login.email
                , onInput SetLoginEmail
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , input
                [ type_ "password"
                , placeholder "Password"
                , value login.password
                , onInput SetLoginPassword
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , button
                [ type_ "submit"
                , disabled login.loading
                , class "w-full bg-brand text-white py-2 rounded hover:bg-brand-dark disabled:opacity-50"
                ]
                [ text
                    (if login.loading then
                        "Signing in..."

                     else
                        "Sign In"
                    )
                ]
            ]
        ]


viewRegister : RegisterModel -> Html Msg
viewRegister reg =
    div [ class "max-w-md mx-auto" ]
        [ h1 [ class "text-2xl font-bold text-center mb-2" ] [ text "Sign Up" ]
        , p [ class "text-center text-sm text-gray-500 mb-6" ]
            [ a [ href "/login", class "text-brand" ] [ text "Have an account?" ] ]
        , Html.form [ onSubmit SubmitRegister, class "space-y-4" ]
            [ input
                [ type_ "text"
                , placeholder "Username"
                , value reg.username
                , onInput SetRegisterUsername
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , input
                [ type_ "email"
                , placeholder "Email"
                , value reg.email
                , onInput SetRegisterEmail
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , input
                [ type_ "password"
                , placeholder "Password"
                , value reg.password
                , onInput SetRegisterPassword
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , button
                [ type_ "submit"
                , disabled reg.loading
                , class "w-full bg-brand text-white py-2 rounded hover:bg-brand-dark disabled:opacity-50"
                ]
                [ text
                    (if reg.loading then
                        "Creating account..."

                     else
                        "Sign Up"
                    )
                ]
            ]
        ]


viewArticleDetail : Model -> ArticleDetailModel -> Html Msg
viewArticleDetail model detail =
    case detail.article of
        Nothing ->
            p [ class "text-gray-500" ] [ text "Loading..." ]

        Just article ->
            div []
                [ div [ class "bg-gray-800 text-white -mx-4 px-4 py-8 mb-8 rounded-lg" ]
                    [ h1 [ class "text-3xl font-bold mb-3" ] [ text article.title ]
                    , div [ class "flex items-center gap-2 text-sm" ]
                        [ a [ href ("/profile/" ++ article.author.username), class "text-green-300" ]
                            [ text article.author.username ]
                        , span [ class "text-gray-400" ] [ text article.createdAt ]
                        ]
                    ]
                , div [ class "prose max-w-none mb-8" ]
                    [ p [] [ text article.body ] ]
                , div [ class "flex gap-1 mb-8" ]
                    (List.map
                        (\tag -> span [ class "text-xs text-gray-400 border rounded px-2 py-0.5" ] [ text tag ])
                        article.tagList
                    )
                , hr [ class "mb-8" ] []
                , h3 [ class "font-semibold mb-4" ] [ text "Comments" ]
                , case model.token of
                    Just _ ->
                        Html.form
                            [ onSubmit (SubmitComment article.slug)
                            , class "mb-6"
                            ]
                            [ textarea
                                [ placeholder "Write a comment..."
                                , value detail.newComment
                                , onInput SetNewComment
                                , class "w-full border rounded px-3 py-2 mb-2"
                                , Html.Attributes.rows 3
                                ]
                                []
                            , button
                                [ type_ "submit"
                                , class "bg-brand text-white text-sm px-4 py-1.5 rounded hover:bg-brand-dark"
                                ]
                                [ text "Post Comment" ]
                            ]

                    Nothing ->
                        p [ class "text-sm text-gray-500 mb-6" ]
                            [ a [ href "/login", class "text-brand" ] [ text "Sign in" ]
                            , text " to add comments."
                            ]
                , div [] (List.map viewComment detail.comments)
                ]


viewComment : Api.Comment -> Html Msg
viewComment comment =
    div [ class "border rounded p-4 mb-3" ]
        [ p [ class "mb-2" ] [ text comment.body ]
        , div [ class "text-xs text-gray-400 flex gap-2" ]
            [ a [ href ("/profile/" ++ comment.author.username), class "text-brand" ]
                [ text comment.author.username ]
            , text comment.createdAt
            ]
        ]


viewNewArticle : NewArticleModel -> Html Msg
viewNewArticle na =
    div [ class "max-w-2xl mx-auto" ]
        [ h1 [ class "text-2xl font-bold mb-6" ] [ text "New Article" ]
        , Html.form [ onSubmit SubmitArticle, class "space-y-4" ]
            [ input
                [ type_ "text"
                , placeholder "Article Title"
                , value na.title
                , onInput SetArticleTitle
                , class "w-full border rounded px-3 py-2 text-lg"
                ]
                []
            , input
                [ type_ "text"
                , placeholder "What's this article about?"
                , value na.description
                , onInput SetArticleDescription
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , textarea
                [ placeholder "Write your article (in markdown)"
                , value na.body
                , onInput SetArticleBody
                , class "w-full border rounded px-3 py-2"
                , Html.Attributes.rows 12
                ]
                []
            , input
                [ type_ "text"
                , placeholder "Enter tags (comma separated)"
                , value na.tags
                , onInput SetArticleTags
                , class "w-full border rounded px-3 py-2"
                ]
                []
            , button
                [ type_ "submit"
                , disabled na.loading
                , class "bg-brand text-white px-6 py-2 rounded hover:bg-brand-dark disabled:opacity-50"
                ]
                [ text
                    (if na.loading then
                        "Publishing..."

                     else
                        "Publish Article"
                    )
                ]
            ]
        ]


viewProfile : ProfileModel -> Html Msg
viewProfile prof =
    case prof.profile of
        Nothing ->
            p [ class "text-gray-500" ] [ text "Loading..." ]

        Just profile ->
            div []
                [ div [ class "text-center py-8 border-b mb-8" ]
                    [ h1 [ class "text-2xl font-bold" ] [ text profile.username ]
                    , case profile.bio of
                        Just bio ->
                            p [ class "text-gray-500 mt-2" ] [ text bio ]

                        Nothing ->
                            text ""
                    ]
                , div []
                    (if List.isEmpty prof.articles then
                        [ p [ class "text-gray-500" ] [ text "No articles yet." ] ]

                     else
                        List.map viewArticlePreview prof.articles
                    )
                ]


onSubmit : Msg -> Attribute Msg
onSubmit msg =
    Html.Events.preventDefaultOn "submit" (D.succeed ( msg, True ))
